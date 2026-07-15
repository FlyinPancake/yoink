//! SoulSeek music provider implementation.
//!
//! Searches for tracks via the slskd REST API, scores candidates by metadata
//! similarity and quality, downloads the best match, and returns the local
//! file path for playback.

pub(crate) mod matching;
pub(crate) mod models;
mod slskd_types;
pub(crate) mod transfer;
pub(crate) mod util;

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use async_trait::async_trait;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use snafu::prelude::*;
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, warn};
use url::Url;

use crate::{
    db::{provider::Provider, quality::Quality},
    providers::*,
};

use self::{
    matching::{
        choose_manual_album_file, rank_album_bundles, rank_album_folders, rank_all_files,
        rank_candidates,
    },
    models::*,
    transfer::{is_complete_success, is_failure},
    util::{dedup_queries, normalize, percent_encode_path, sanitize_relative_path},
};
use super::{DownloadTrackContext, PlaybackInfo, ProviderError, SearchTrackResolver};

/// How many ranked candidates to attempt before giving up on a track.
const MAX_DOWNLOAD_ATTEMPTS: usize = 3;

// ── Source ───────────────────────────────────────────────────────────

pub(crate) struct SoulSeekSource {
    http: reqwest::Client,
    slskd_base_url: Url,
    username: String,
    password: String,
    downloads_dir: PathBuf,
    token: RwLock<Option<String>>,
    /// slskd allows only one concurrent `POST /searches` operation.
    search_request_gate: Semaphore,
    /// slskd also allows only one concurrent download-enqueue operation.
    transfer_request_gate: Semaphore,
}

impl SoulSeekSource {
    pub fn new(
        http: reqwest::Client,
        mut slskd_base_url: Url,
        username: String,
        password: String,
        downloads_dir: String,
    ) -> Self {
        // `Url::join` treats a base path without a trailing slash as a file.
        // Normalize it once so reverse-proxy prefixes such as `/slskd` are kept.
        if !slskd_base_url.path().ends_with('/') {
            let path = format!("{}/", slskd_base_url.path());
            slskd_base_url.set_path(&path);
        }
        slskd_base_url.set_query(None);
        slskd_base_url.set_fragment(None);

        Self {
            http,
            slskd_base_url,
            username: username.trim().to_string(),
            password: password.trim().to_string(),
            downloads_dir: PathBuf::from(downloads_dir.trim()),
            token: RwLock::new(None),
            search_request_gate: Semaphore::new(1),
            transfer_request_gate: Semaphore::new(1),
        }
    }
}

// ── SearchTrackResolver trait ──────────────────────────────────────

#[async_trait]
impl SearchTrackResolver for SoulSeekSource {
    fn id(&self) -> Provider {
        Provider::Soulseek
    }

    async fn resolve(
        &self,
        ctx: &DownloadTrackContext,
        quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        // Try album-bundle search first, then fall back to per-track search.
        // Each phase yields ranked candidates; failed or unverifiable
        // downloads fall through to the next candidate.
        let bundle_candidates = self.find_album_bundle_candidates(ctx, quality).await?;
        let (path, mut last_error) = self.try_candidates(bundle_candidates, ctx).await;
        if let Some(path) = path {
            return Ok(PlaybackInfo::LocalFile(path));
        }

        match self.find_single_track_candidates(ctx, quality).await {
            Ok(candidates) => {
                let (path, error) = self.try_candidates(candidates, ctx).await;
                if let Some(path) = path {
                    return Ok(PlaybackInfo::LocalFile(path));
                }
                last_error = error.or(last_error);
            }
            // A transfer failure from the bundle phase is more informative
            // than "no single-track candidate found".
            Err(error) => last_error = last_error.or(Some(error)),
        }

        Err(last_error.unwrap_or_else(|| {
            NotFoundSnafu {
                provider: Provider::Soulseek,
                resource: format!("suitable candidate for '{}'", ctx.track_title),
            }
            .build()
        }))
    }

    async fn manual_search(
        &self,
        ctx: &DownloadTrackContext,
        quality: &Quality,
    ) -> Result<Vec<ManualSearchCandidate>, ProviderError> {
        // Merge album-level and track-level search results so the user sees
        // both loose files and complete album folders.
        let mut responses = self.search_album_queries(ctx, quality).await?;
        match self.search_track_queries(ctx).await {
            Ok(track_responses) => responses.extend(track_responses),
            Err(error) => {
                if responses.is_empty() {
                    return Err(error);
                }
                warn!(error = %error, "manual track search failed; using album results only");
            }
        }

        Ok(rank_all_files(&responses, ctx, quality))
    }

    async fn fetch_file(
        &self,
        selection: &ManualDownloadSelection,
    ) -> Result<PlaybackInfo, ProviderError> {
        self.fetch_chosen_file(&selection.username, &selection.filename, selection.size)
            .await
    }

    async fn manual_album_search(
        &self,
        tracks: &[DownloadTrackContext],
        quality: &Quality,
    ) -> Result<Vec<ManualAlbumCandidate>, ProviderError> {
        let Some(first) = tracks.first() else {
            return Ok(Vec::new());
        };
        let responses = self.search_album_queries(first, quality).await?;
        Ok(rank_album_folders(&responses, tracks, quality))
    }

    async fn fetch_album_file(
        &self,
        selection: &ManualAlbumSelection,
        ctx: &DownloadTrackContext,
        quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        let chosen = choose_manual_album_file(&selection.files, &selection.username, ctx, quality)
            .context(NotFoundSnafu {
                provider: Provider::Soulseek,
                resource: format!(
                    "file matching '{}' in chosen folder {}",
                    ctx.track_title, selection.folder
                ),
            })?;

        self.fetch_chosen_file(&selection.username, &chosen.filename, chosen.size)
            .await
    }
}

// ── High-level search strategies ────────────────────────────────────

impl SoulSeekSource {
    async fn find_album_bundle_candidates(
        &self,
        ctx: &DownloadTrackContext,
        quality: &Quality,
    ) -> Result<Vec<matching::Candidate>, ProviderError> {
        let responses = self.search_album_queries(ctx, quality).await?;
        if responses.is_empty() {
            return Ok(Vec::new());
        }
        let candidates = rank_album_bundles(&responses, ctx, quality);
        debug!(
            track = ctx.track_title,
            responses = responses.len(),
            files = responses.iter().map(|r| r.files.len()).sum::<usize>(),
            candidates = candidates.len(),
            "SoulSeek album bundle ranking"
        );
        Ok(candidates)
    }

    async fn find_single_track_candidates(
        &self,
        ctx: &DownloadTrackContext,
        quality: &Quality,
    ) -> Result<Vec<matching::Candidate>, ProviderError> {
        let responses = self.search_track_queries(ctx).await?;

        ensure!(
            !responses.is_empty(),
            NotFoundSnafu {
                provider: Provider::Soulseek,
                resource: format!("search responses for track '{}'", ctx.track_title),
            }
        );

        let candidates = rank_candidates(&responses, ctx, quality);
        debug!(
            track = ctx.track_title,
            responses = responses.len(),
            files = responses.iter().map(|r| r.files.len()).sum::<usize>(),
            candidates = candidates.len(),
            "SoulSeek single-track ranking"
        );
        ensure!(
            !candidates.is_empty(),
            NotFoundSnafu {
                provider: Provider::Soulseek,
                resource: format!("suitable candidate for '{}'", ctx.track_title),
            }
        );
        Ok(candidates)
    }

    /// Attempt up to [`MAX_DOWNLOAD_ATTEMPTS`] candidates in ranked order.
    /// Returns the first verified local file, plus the last error when every
    /// attempt failed.
    async fn try_candidates(
        &self,
        candidates: Vec<matching::Candidate>,
        ctx: &DownloadTrackContext,
    ) -> (Option<PathBuf>, Option<ProviderError>) {
        let mut last_error = None;

        for candidate in candidates.into_iter().take(MAX_DOWNLOAD_ATTEMPTS) {
            match self.download_and_verify(&candidate, ctx).await {
                Ok(path) => return (Some(path), None),
                Err(error) => {
                    warn!(
                        username = candidate.username,
                        filename = candidate.filename,
                        error = %error,
                        "SoulSeek candidate failed; trying next"
                    );
                    last_error = Some(error);
                }
            }
        }

        (None, last_error)
    }

    /// Download a specific user-chosen file. Only checks the result is
    /// readable audio — no duration second-guessing, the user picked it
    /// deliberately.
    async fn fetch_chosen_file(
        &self,
        username: &str,
        filename: &str,
        size: i64,
    ) -> Result<PlaybackInfo, ProviderError> {
        debug!(username, filename, "SoulSeek manual download enqueueing");
        self.enqueue_download(username, filename, size).await?;
        let local_path = self.wait_for_download(username, filename, 300).await?;
        probe_audio_duration(&local_path).await?;
        Ok(PlaybackInfo::LocalFile(local_path))
    }

    async fn download_and_verify(
        &self,
        candidate: &matching::Candidate,
        ctx: &DownloadTrackContext,
    ) -> Result<PathBuf, ProviderError> {
        debug!(
            username = candidate.username,
            filename = candidate.filename,
            score = candidate.score,
            "SoulSeek download enqueueing"
        );
        self.enqueue_download(&candidate.username, &candidate.filename, candidate.size)
            .await?;
        let local_path = self
            .wait_for_download(&candidate.username, &candidate.filename, 180)
            .await?;
        verify_downloaded_file(&local_path, candidate, ctx).await?;
        debug!(
            filename = candidate.filename,
            path = %local_path.display(),
            "SoulSeek download verified"
        );
        Ok(local_path)
    }

    /// Build track-level queries from most precise to broadest and return the
    /// first search that yields results.
    async fn search_track_queries(
        &self,
        ctx: &DownloadTrackContext,
    ) -> Result<Vec<SearchResponse>, ProviderError> {
        let artist = ctx.artist_name.trim();
        let album = ctx.album_title.trim();
        let track = ctx.track_title.trim();

        let mut queries = vec![
            format!("{artist} {album} {track}"),
            format!("{artist} {track}"),
            format!("{track} {artist}"),
            format!("{track} {album}"),
            track.to_string(),
        ];

        // Add a normalized variant with punctuation removed for troublesome titles.
        let track_norm = normalize(track);
        if !track_norm.is_empty() && track_norm != track.to_ascii_lowercase() {
            queries.push(track_norm);
        }

        self.run_first_successful_search(queries).await
    }

    /// Build album-level queries and return the first search that yields results.
    async fn search_album_queries(
        &self,
        ctx: &DownloadTrackContext,
        quality: &Quality,
    ) -> Result<Vec<SearchResponse>, ProviderError> {
        let expected_tracks = ctx.album_track_count.unwrap_or(0);
        if expected_tracks == 0 {
            return Ok(Vec::new());
        }

        let artist = ctx.artist_name.trim();
        let album = ctx.album_title.trim();
        if artist.is_empty() || album.is_empty() {
            return Ok(Vec::new());
        }

        let quality_hint = match quality {
            Quality::HiRes | Quality::Lossless => "flac",
            _ => "mp3",
        };

        let mut queries = vec![
            format!("{artist} {album}"),
            format!("{album} {artist}"),
            format!("{artist} {album} {quality_hint}"),
        ];

        let album_norm = normalize(album);
        if !album_norm.is_empty() && album_norm != album.to_ascii_lowercase() {
            queries.push(format!("{artist} {album_norm}"));
        }

        self.run_first_successful_search(queries).await
    }

    /// Deduplicate `queries`, execute each in order, and return the first
    /// non-empty set of responses (or an empty vec if all come back empty).
    async fn run_first_successful_search(
        &self,
        queries: Vec<String>,
    ) -> Result<Vec<SearchResponse>, ProviderError> {
        for query in dedup_queries(queries) {
            let search = self.start_search(&query).await?;
            let responses = self.poll_search_responses(&search.id, 75).await;
            if let Err(error) = self.delete_search(&search.id).await {
                warn!(
                    search_id = search.id,
                    error = %error,
                    "failed to clean up SoulSeek search"
                );
            }
            let responses = responses?;
            if !responses.is_empty() {
                debug!(query = %query, count = responses.len(), "SoulSeek search hit");
                return Ok(responses);
            }
            debug!(query = %query, "SoulSeek search returned no responses");
        }
        Ok(Vec::new())
    }
}

// ── slskd API interaction ───────────────────────────────────────────

impl SoulSeekSource {
    fn endpoint(&self, path: &str) -> Result<Url, ProviderError> {
        self.slskd_base_url
            .join(path.trim_start_matches('/'))
            .map_err(|error| ProviderError::InvalidResponse {
                provider: Provider::Soulseek,
                reason: format!("invalid slskd API path {path}: {error}"),
            })
    }

    async fn auth_token(&self) -> Result<Option<String>, ProviderError> {
        if self.username.is_empty() || self.password.is_empty() {
            return Ok(None);
        }

        if let Some(token) = self.token.read().await.clone() {
            return Ok(Some(token));
        }

        let url = self.endpoint("/api/v0/session")?;
        let payload = LoginRequest {
            username: Some(self.username.clone()),
            password: Some(self.password.clone()),
        };

        let resp = self
            .http
            .post(url)
            .json(&payload)
            .send()
            .await
            .context(ReqwestSnafu {
                provider: Provider::Soulseek,
                operation: "slskd login request",
            })?;

        ensure!(
            resp.status().is_success(),
            AuthSnafu {
                provider: Provider::Soulseek,
                reason: format!("slskd login failed with status {}", resp.status()),
            }
        );

        let token_resp: TokenResponse = resp.json().await.context(ReqwestSnafu {
            provider: Provider::Soulseek,
            operation: "slskd login response".to_string(),
        })?;

        let token = token_resp.token.context(AuthSnafu {
            provider: Provider::Soulseek,
            reason: "slskd login response did not contain a token",
        })?;
        *self.token.write().await = Some(token.clone());
        Ok(Some(token))
    }

    /// Authenticated POST that deserializes a JSON response.
    async fn post_json<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ProviderError> {
        let url = self.endpoint(path)?;
        let mut auth_retry = false;
        let resp = loop {
            let token = self.auth_token().await?;
            let mut req = self
                .http
                .post(url.clone())
                .json(body)
                .timeout(Duration::from_secs(30));
            if let Some(token) = &token {
                req = req.bearer_auth(token);
            }

            let resp = req.send().await.context(ReqwestSnafu {
                provider: Provider::Soulseek,
                operation: format!("slskd POST {path}"),
            })?;
            if !auth_retry && token.is_some() && is_auth_rejection(resp.status()) {
                self.invalidate_token(token.as_deref()).await;
                auth_retry = true;
                continue;
            }
            break resp;
        };

        if !resp.status().is_success() {
            return Err(response_error(resp, &format!("slskd POST {path}")).await);
        }

        resp.json().await.context(ReqwestSnafu {
            provider: Provider::Soulseek,
            operation: format!("slskd POST {path} decode"),
        })
    }

    /// Authenticated GET that deserializes a JSON response.
    async fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, ProviderError> {
        let url = self.endpoint(path)?;
        let mut auth_retry = false;
        let resp = loop {
            let token = self.auth_token().await?;
            let mut req = self.http.get(url.clone()).timeout(Duration::from_secs(30));
            if let Some(token) = &token {
                req = req.bearer_auth(token);
            }

            let resp = req.send().await.context(ReqwestSnafu {
                provider: Provider::Soulseek,
                operation: format!("slskd GET {path}"),
            })?;
            if !auth_retry && token.is_some() && is_auth_rejection(resp.status()) {
                self.invalidate_token(token.as_deref()).await;
                auth_retry = true;
                continue;
            }
            break resp;
        };

        if !resp.status().is_success() {
            return Err(response_error(resp, &format!("slskd GET {path}")).await);
        }

        resp.json().await.context(ReqwestSnafu {
            provider: Provider::Soulseek,
            operation: format!("slskd GET {path} decode"),
        })
    }

    async fn invalidate_token(&self, rejected_token: Option<&str>) {
        let mut cached = self.token.write().await;
        if cached.as_deref() == rejected_token {
            *cached = None;
        }
    }

    async fn delete_search(&self, search_id: &str) -> Result<(), ProviderError> {
        let path = format!("/api/v0/searches/{search_id}");
        let url = self.endpoint(&path)?;
        let mut auth_retry = false;
        let resp = loop {
            let token = self.auth_token().await?;
            let mut req = self
                .http
                .delete(url.clone())
                .timeout(Duration::from_secs(30));
            if let Some(token) = &token {
                req = req.bearer_auth(token);
            }

            let resp = req.send().await.context(ReqwestSnafu {
                provider: Provider::Soulseek,
                operation: "slskd search cleanup",
            })?;
            if !auth_retry && token.is_some() && is_auth_rejection(resp.status()) {
                self.invalidate_token(token.as_deref()).await;
                auth_retry = true;
                continue;
            }
            break resp;
        };
        if resp.status().is_success() || resp.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        Err(response_error(resp, &format!("slskd DELETE {path}")).await)
    }

    /// Kick off a search, retrying on 429 rate-limit responses.
    async fn start_search(&self, query: &str) -> Result<Search, ProviderError> {
        let Ok(_permit) = self.search_request_gate.acquire().await else {
            return UnavailableSnafu {
                provider: Provider::Soulseek,
                reason: "search gate closed",
            }
            .fail();
        };

        let req = SearchRequest {
            search_text: Some(query.to_string()),
            ..Default::default()
        };

        let mut delay_secs = 1u64;
        for attempt in 1..=5 {
            match self.post_json("/api/v0/searches", &req).await {
                Ok(search) => return Ok(search),
                Err(err) if is_rate_limited(&err) && attempt < 5 => {
                    warn!(
                        query,
                        attempt, delay_secs, "SoulSeek search rate-limited; retrying"
                    );
                    tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                    delay_secs = (delay_secs * 2).min(8);
                }
                Err(err) => return Err(err),
            }
        }

        RateLimitedSnafu {
            provider: Provider::Soulseek,
            reason: "search creation failed after retries",
        }
        .fail()
    }

    /// Poll until search completes or `timeout_secs` elapses.
    async fn poll_search_responses(
        &self,
        search_id: &str,
        timeout_secs: u64,
    ) -> Result<Vec<SearchResponse>, ProviderError> {
        let state_path = format!("/api/v0/searches/{search_id}");
        let responses_path = format!("/api/v0/searches/{search_id}/responses");
        let mut has_responses = false;
        let mut elapsed = 0u64;

        while elapsed < timeout_secs {
            let status: SearchStatus = self.get_json(&state_path).await?;

            if status.response_count > 0 {
                has_responses = true;
                let responses: Vec<SearchResponse> = self.get_json(&responses_path).await?;
                if !responses.is_empty() {
                    return Ok(responses);
                }
            }

            if status.is_complete {
                // slskd may only materialize response payloads near completion.
                if has_responses {
                    return self.get_json(&responses_path).await;
                }
                return Ok(Vec::new());
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
            elapsed += 2;
        }

        // Final attempt after timeout if we ever saw a non-zero response count.
        if has_responses {
            return self.get_json(&responses_path).await;
        }
        Ok(Vec::new())
    }
}

// ── Download / transfer ─────────────────────────────────────────────

impl SoulSeekSource {
    /// Enqueue a download, serialized through the transfer gate and retried
    /// with backoff: slskd rejects concurrent enqueue operations with 429.
    async fn enqueue_download(
        &self,
        username: &str,
        filename: &str,
        size: i64,
    ) -> Result<(), ProviderError> {
        let Ok(_permit) = self.transfer_request_gate.acquire().await else {
            return UnavailableSnafu {
                provider: Provider::Soulseek,
                reason: "transfer gate closed",
            }
            .fail();
        };

        let mut delay_secs = 1u64;
        for attempt in 1..=6 {
            match self.enqueue_download_once(username, filename, size).await {
                Ok(()) => return Ok(()),
                Err(err) if is_rate_limited(&err) && attempt < 6 => {
                    warn!(
                        username,
                        filename, attempt, delay_secs, "slskd enqueue rate-limited; retrying"
                    );
                    tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                    delay_secs = (delay_secs * 2).min(8);
                }
                Err(err) => return Err(err),
            }
        }

        RateLimitedSnafu {
            provider: Provider::Soulseek,
            reason: "download enqueue failed after retries",
        }
        .fail()
    }

    async fn enqueue_download_once(
        &self,
        username: &str,
        filename: &str,
        size: i64,
    ) -> Result<(), ProviderError> {
        let path = format!(
            "/api/v0/transfers/downloads/{}",
            percent_encode_path(username)
        );
        let body = vec![QueueDownloadRequest {
            filename: filename.to_string(),
            size: Some(size),
        }];

        let url = self.endpoint(&path)?;
        let mut auth_retry = false;
        let resp = loop {
            let token = self.auth_token().await?;
            let mut req = self
                .http
                .post(url.clone())
                .json(&body)
                .timeout(Duration::from_secs(30));
            if let Some(token) = &token {
                req = req.bearer_auth(token);
            }

            let resp = req.send().await.context(ReqwestSnafu {
                provider: Provider::Soulseek,
                operation: "enqueue download",
            })?;
            if !auth_retry && token.is_some() && is_auth_rejection(resp.status()) {
                self.invalidate_token(token.as_deref()).await;
                auth_retry = true;
                continue;
            }
            break resp;
        };

        if !resp.status().is_success() {
            return Err(response_error(resp, "slskd enqueue download").await);
        }

        Ok(())
    }

    async fn wait_for_download(
        &self,
        username: &str,
        filename: &str,
        timeout_secs: u64,
    ) -> Result<PathBuf, ProviderError> {
        let path = format!(
            "/api/v0/transfers/downloads/{}",
            percent_encode_path(username)
        );
        let mut elapsed = 0u64;

        while elapsed < timeout_secs {
            let transfer_user: TransferUserResponse = self.get_json(&path).await?;
            let mut found = false;

            for dir in &transfer_user.directories {
                for file in &dir.files {
                    if file.filename != filename {
                        continue;
                    }
                    found = true;

                    if is_failure(file) {
                        let detail = file
                            .exception
                            .clone()
                            .or_else(|| file.state_description.clone())
                            .unwrap_or_else(|| "unknown transfer failure".to_string());
                        return UnavailableSnafu {
                            provider: Provider::Soulseek,
                            reason: format!("transfer failed for {filename}: {detail}"),
                        }
                        .fail();
                    }

                    if is_complete_success(file)
                        && let Some(local) = self
                            .find_local_file(dir.directory.as_deref(), &file.filename)
                            .await
                    {
                        return Ok(local);
                    }
                }
            }

            if found {
                debug!(username, filename, "soulseek transfer in progress");
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
            elapsed += 2;
        }

        UnavailableSnafu {
            provider: Provider::Soulseek,
            reason: format!("timed out waiting for download: {filename}"),
        }
        .fail()
    }

    /// Check candidate local paths for a completed download.
    async fn find_local_file(
        &self,
        directory: Option<&str>,
        slsk_filename: &str,
    ) -> Option<PathBuf> {
        for candidate in self.resolve_local_download_paths(directory, slsk_filename) {
            if tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
                return Some(candidate);
            }
        }
        None
    }

    fn resolve_local_download_paths(
        &self,
        directory: Option<&str>,
        slsk_filename: &str,
    ) -> Vec<PathBuf> {
        let mut out = Vec::new();

        let file_path = sanitize_relative_path(slsk_filename);
        out.push(self.downloads_dir.join(&file_path));
        if let Some(name) = Path::new(&file_path).file_name() {
            out.push(self.downloads_dir.join(name));
        }

        if let Some(dir) = directory {
            let dir_path = sanitize_relative_path(dir);
            if let Some(name) = Path::new(&file_path).file_name() {
                out.push(self.downloads_dir.join(&dir_path).join(name));
                if let Some(leaf) = Path::new(&dir_path).file_name() {
                    out.push(self.downloads_dir.join(leaf).join(name));
                }
            }
        }

        out.sort();
        out.dedup();
        out
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Read the real duration of a downloaded file, failing if it is not
/// parseable audio at all.
async fn probe_audio_duration(path: &Path) -> Result<Duration, ProviderError> {
    use lofty::{file::AudioFile, probe::Probe};

    let probe_path = path.to_path_buf();
    let probed = tokio::task::spawn_blocking(move || {
        Probe::open(&probe_path)
            .and_then(|probe| probe.read())
            .map(|file| file.properties().duration())
    })
    .await;

    match probed {
        Ok(Ok(duration)) => Ok(duration),
        Ok(Err(error)) => InvalidResponseSnafu {
            provider: Provider::Soulseek,
            reason: format!(
                "downloaded file {} is not readable audio: {error}",
                path.display()
            ),
        }
        .fail(),
        Err(error) => InvalidResponseSnafu {
            provider: Provider::Soulseek,
            reason: format!("audio probe task failed for {}: {error}", path.display()),
        }
        .fail(),
    }
}

/// Probe the downloaded file and check it is readable audio with a credible
/// duration, so a bad grab falls through to the next candidate instead of
/// landing in the library.
///
/// The primary duration reference is what the peer advertised in the search
/// result — matching already vetted that value, and comparing against it
/// catches truncated transfers and lying peers. Only when nothing was
/// advertised do we fall back to the catalog duration, generously: it can be
/// legitimately far off for the same recording (megamix segment boundaries,
/// pressing differences).
async fn verify_downloaded_file(
    path: &Path,
    candidate: &matching::Candidate,
    ctx: &DownloadTrackContext,
) -> Result<(), ProviderError> {
    let duration = probe_audio_duration(path).await?;

    let actual = u32::try_from(duration.as_secs()).unwrap_or(u32::MAX);
    if let Some(reported) = candidate.reported_length {
        ensure!(
            actual.abs_diff(reported) <= 10,
            InvalidResponseSnafu {
                provider: Provider::Soulseek,
                reason: format!(
                    "downloaded file {} lasts {actual}s but the peer advertised {reported}s",
                    path.display()
                ),
            }
        );
    } else if let Some(expected) = ctx.duration_secs {
        let tolerance = 45.max(expected / 3);
        ensure!(
            actual.abs_diff(expected) <= tolerance,
            InvalidResponseSnafu {
                provider: Provider::Soulseek,
                reason: format!(
                    "downloaded file {} lasts {actual}s but roughly {expected}s was expected",
                    path.display()
                ),
            }
        );
    }

    Ok(())
}

fn is_rate_limited(err: &ProviderError) -> bool {
    if matches!(err, ProviderError::RateLimited { .. }) {
        return true;
    }
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("429") || msg.contains("too many requests")
}

fn is_auth_rejection(status: StatusCode) -> bool {
    matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
}

async fn response_error(resp: reqwest::Response, operation: &str) -> ProviderError {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let body = body.trim();
    let reason = if body.is_empty() {
        format!("{operation} returned {status}")
    } else {
        let truncated: String = body.chars().take(500).collect();
        format!("{operation} returned {status}: {truncated}")
    };

    if status == StatusCode::TOO_MANY_REQUESTS {
        ProviderError::RateLimited {
            provider: Provider::Soulseek,
            reason,
        }
    } else if is_auth_rejection(status) {
        ProviderError::Auth {
            provider: Provider::Soulseek,
            reason,
        }
    } else {
        ProviderError::InvalidResponse {
            provider: Provider::Soulseek,
            reason,
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_context(
        track_title: &str,
        track_number: u32,
        album_track_count: usize,
    ) -> DownloadTrackContext {
        DownloadTrackContext {
            artist_name: "The Artist".to_string(),
            album_title: "The Album".to_string(),
            track_title: track_title.to_string(),
            track_number: Some(track_number),
            album_track_count: Some(album_track_count),
            duration_secs: None,
        }
    }

    fn search_file(filename: &str, size: i64) -> SearchFile {
        SearchFile {
            filename: filename.to_string(),
            size,
            length: None,
            bit_rate: None,
            extension: Path::new(filename)
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string()),
        }
    }

    fn search_response(username: &str, files: Vec<SearchFile>) -> SearchResponse {
        SearchResponse {
            username: username.to_string(),
            files,
            has_free_upload_slot: false,
            queue_length: 0,
            upload_speed: 0,
        }
    }

    fn test_source(base_url: &str) -> SoulSeekSource {
        SoulSeekSource::new(
            reqwest::Client::new(),
            base_url.parse().expect("slskd base url parse"),
            "".to_string(),
            "".to_string(),
            "/tmp/slskd-downloads".to_string(),
        )
    }

    #[test]
    fn endpoint_does_not_add_a_double_slash() {
        let source = test_source("http://127.0.0.1:5030");

        assert_eq!(
            source.endpoint("/api/v0/searches").unwrap().as_str(),
            "http://127.0.0.1:5030/api/v0/searches"
        );
    }

    #[test]
    fn endpoint_preserves_reverse_proxy_prefix() {
        let source = test_source("https://example.com/slskd");

        assert_eq!(
            source.endpoint("/api/v0/searches").unwrap().as_str(),
            "https://example.com/slskd/api/v0/searches"
        );
    }

    fn transfer_with_state(state: &str) -> Transfer {
        Transfer {
            filename: "track.flac".to_string(),
            state: Some(state.to_string()),
            state_description: Some(state.to_string()),
            exception: None,
            size: Some(100),
            bytes_remaining: Some(0),
            bytes_transferred: Some(100),
        }
    }

    #[test]
    fn transfer_failure_detects_rejected_terminal_state() {
        let t = transfer_with_state("Completed, Rejected");
        assert!(is_failure(&t));
        assert!(!is_complete_success(&t));
    }

    #[test]
    fn transfer_success_detects_completed_succeeded_state() {
        let t = transfer_with_state("Completed, Succeeded");
        assert!(!is_failure(&t));
        assert!(is_complete_success(&t));
    }

    #[test]
    fn transfer_success_detects_byte_completion_without_state_text() {
        let t = Transfer {
            filename: "track.flac".to_string(),
            state: Some("InProgress".to_string()),
            state_description: None,
            exception: None,
            size: Some(500),
            bytes_remaining: Some(0),
            bytes_transferred: Some(500),
        };
        assert!(is_complete_success(&t));
    }

    #[test]
    fn resolve_local_download_paths_includes_leaf_directory_variant() {
        let source = test_source("http://127.0.0.1:5030");

        let paths = source.resolve_local_download_paths(
            Some("audiophile\\ATMOS\\Frank Zappa\\Over-Nite Sensation"),
            "audiophile\\ATMOS\\Frank Zappa\\Over-Nite Sensation\\1-03 Dirty Love.m4a",
        );

        let expected_leaf =
            PathBuf::from("/tmp/slskd-downloads/Over-Nite Sensation/1-03 Dirty Love.m4a");

        assert!(paths.contains(&expected_leaf));
    }

    #[test]
    fn sanitize_relative_path_strips_parent_segments() {
        let cleaned = sanitize_relative_path("../../bad\\../music/track.flac");
        assert_eq!(cleaned, PathBuf::from("bad/music/track.flac"));
    }

    #[test]
    fn album_bundle_selection_requires_complete_track_count() {
        let ctx = test_context("Song Two", 2, 3);
        let responses = vec![search_response(
            "user1",
            vec![
                search_file("The Artist/The Album/01 - Song One.flac", 100),
                search_file("The Artist/The Album/02 - Song Two.flac", 100),
            ],
        )];

        assert!(rank_album_bundles(&responses, &ctx, &Quality::Lossless).is_empty());
    }

    #[test]
    fn album_bundle_selection_picks_requested_track_from_complete_bundle() {
        let ctx = test_context("Song Two", 2, 2);
        let responses = vec![search_response(
            "user1",
            vec![
                search_file("The Artist/The Album/01 - Song One.flac", 100),
                search_file("The Artist/The Album/02 - Song Two.flac", 100),
            ],
        )];

        let candidates = rank_album_bundles(&responses, &ctx, &Quality::Lossless);
        let candidate = candidates
            .first()
            .expect("expected complete album candidate");
        assert_eq!(candidate.username, "user1");
        assert!(candidate.filename.contains("02 - Song Two"));
    }

    #[test]
    fn album_bundle_selection_falls_back_to_title_when_numbering_differs() {
        // The DB says track 2, but this pressing has the song at position 1:
        // the title match must win over the track number.
        let ctx = test_context("Song One", 2, 2);
        let responses = vec![search_response(
            "user1",
            vec![
                search_file("The Artist/The Album/01 - Song One.flac", 100),
                search_file("The Artist/The Album/02 - Interlude.flac", 100),
            ],
        )];

        let candidates = rank_album_bundles(&responses, &ctx, &Quality::Lossless);
        let candidate = candidates.first().expect("expected title-matched track");
        assert!(candidate.filename.contains("01 - Song One"));
    }

    #[test]
    fn album_bundle_selection_rejects_bundle_without_any_title_match() {
        let ctx = test_context("Song Three", 2, 2);
        let responses = vec![search_response(
            "user1",
            vec![
                search_file("The Artist/The Album/01 - Song One.flac", 100),
                search_file("The Artist/The Album/02 - Interlude.flac", 100),
            ],
        )];

        assert!(rank_album_bundles(&responses, &ctx, &Quality::Lossless).is_empty());
    }

    #[test]
    fn album_bundle_selection_rejects_folder_without_artist_or_album_context() {
        let ctx = test_context("Song Two", 2, 2);
        let responses = vec![search_response(
            "user1",
            vec![
                search_file("shared/random stuff/01 - Song One.flac", 100),
                search_file("shared/random stuff/02 - Song Two.flac", 100),
            ],
        )];

        assert!(rank_album_bundles(&responses, &ctx, &Quality::Lossless).is_empty());
    }

    #[test]
    fn single_track_selection_ignores_non_audio_files() {
        let ctx = test_context("Song One", 1, 1);
        let responses = vec![search_response(
            "user1",
            vec![search_file("The Artist/The Album/Song One.jpg", 100)],
        )];

        assert!(rank_candidates(&responses, &ctx, &Quality::Lossless).is_empty());
    }

    #[test]
    fn single_track_selection_detects_extension_from_filename() {
        let ctx = test_context("Song One", 1, 1);
        let mut file = search_file("The Artist/The Album/Song One.FLAC", 100);
        file.extension = None;
        let responses = vec![search_response("user1", vec![file])];

        let candidates = rank_candidates(&responses, &ctx, &Quality::Lossless);
        let candidate = candidates.first().expect("expected audio candidate");
        assert!(candidate.filename.ends_with("Song One.FLAC"));
    }

    fn machine_gun_context() -> DownloadTrackContext {
        DownloadTrackContext {
            artist_name: "Noisia".to_string(),
            album_title: "Split The Atom".to_string(),
            track_title: "Machine Gun".to_string(),
            track_number: Some(1),
            album_track_count: Some(19),
            duration_secs: Some(245),
        }
    }

    #[test]
    fn single_track_selection_rejects_title_substring_from_different_song() {
        let ctx = machine_gun_context();
        let mut wrong = search_file(
            "shared/_Untagged/Klute_Unknown Artist/_Unknown Album/03 - Machine Gun Etiquette.flac",
            50_730_582,
        );
        wrong.length = Some(444);
        let responses = vec![search_response("peer", vec![wrong])];

        assert!(rank_candidates(&responses, &ctx, &Quality::Lossless).is_empty());
    }

    #[test]
    fn single_track_selection_accepts_compatible_title_and_duration() {
        let ctx = machine_gun_context();
        let mut correct = search_file(
            "Noisia/Split The Atom/01 - Noisia - Machine Gun (Original Mix).flac",
            30_000_000,
        );
        correct.length = Some(245);
        let responses = vec![search_response("peer", vec![correct])];

        let candidates = rank_candidates(&responses, &ctx, &Quality::Lossless);
        let candidate = candidates.first().expect("expected compatible candidate");
        assert!(candidate.filename.contains("Machine Gun (Original Mix)"));
    }

    #[test]
    fn single_track_selection_rejects_wrong_version_markers() {
        let ctx = machine_gun_context();
        let mut instrumental = search_file(
            "Noisia/Split The Atom/01 - Noisia - Machine Gun (Instrumental).flac",
            30_000_000,
        );
        instrumental.length = Some(245);
        let responses = vec![search_response("peer", vec![instrumental])];

        assert!(rank_candidates(&responses, &ctx, &Quality::Lossless).is_empty());
    }

    #[test]
    fn single_track_selection_allows_version_marker_requested_in_title() {
        let ctx = DownloadTrackContext {
            track_title: "Machine Gun (16 Bit Remix)".to_string(),
            ..machine_gun_context()
        };
        let mut remix = search_file(
            "Noisia/Split The Atom/19 - Machine Gun (16 Bit Remix).flac",
            30_000_000,
        );
        remix.length = Some(245);
        let responses = vec![search_response("peer", vec![remix])];

        assert!(!rank_candidates(&responses, &ctx, &Quality::Lossless).is_empty());
    }

    #[test]
    fn single_track_selection_forgives_one_missing_token_with_context() {
        // Title "Machine Gun Part 2" vs filename "... Machine Gun Pt 2 ...":
        // one token differs, but artist context and duration line up.
        let ctx = DownloadTrackContext {
            track_title: "Machine Gun Part 2".to_string(),
            ..machine_gun_context()
        };
        let mut close = search_file(
            "Noisia/Split The Atom/02 - Noisia - Machine Gun Pt 2.flac",
            30_000_000,
        );
        close.length = Some(245);
        let responses = vec![search_response("peer", vec![close])];

        assert!(!rank_candidates(&responses, &ctx, &Quality::Lossless).is_empty());
    }

    #[test]
    fn single_track_selection_ranks_multiple_candidates_best_first() {
        let ctx = machine_gun_context();
        let mut flac = search_file("Noisia/Split The Atom/01 - Machine Gun.flac", 30_000_000);
        flac.length = Some(245);
        let mut mp3 = search_file("Noisia/Split The Atom/01 - Machine Gun.mp3", 8_000_000);
        mp3.length = Some(245);
        let responses = vec![search_response("peer", vec![mp3, flac])];

        let candidates = rank_candidates(&responses, &ctx, &Quality::Lossless);
        assert_eq!(candidates.len(), 2);
        assert!(candidates[0].filename.ends_with(".flac"));
        assert!(candidates[0].score > candidates[1].score);
    }

    #[test]
    fn single_track_selection_matches_joined_and_split_words() {
        // "Sky High" requested, file says "Skyhigh" — and vice versa.
        let ctx = DownloadTrackContext {
            track_title: "Sky High".to_string(),
            ..machine_gun_context()
        };
        let mut joined = search_file("Noisia/Split The Atom/02 - Skyhigh.flac", 30_000_000);
        joined.length = Some(245);
        let responses = vec![search_response("peer", vec![joined])];
        assert!(!rank_candidates(&responses, &ctx, &Quality::Lossless).is_empty());

        let ctx = DownloadTrackContext {
            track_title: "Skyhigh".to_string(),
            ..machine_gun_context()
        };
        let mut split = search_file("Noisia/Split The Atom/02 - Sky High.flac", 30_000_000);
        split.length = Some(245);
        let responses = vec![search_response("peer", vec![split])];
        assert!(!rank_candidates(&responses, &ctx, &Quality::Lossless).is_empty());
    }

    #[test]
    fn single_track_selection_compact_match_rejects_unrelated_leftover() {
        // "Machine Gun" is a compact substring of "Machine Gun Etiquette",
        // but the leftover is another song's title, not noise.
        let ctx = machine_gun_context();
        let mut wrong = search_file("random/03 - MachineGun Etiquette.flac", 30_000_000);
        wrong.length = None;
        let responses = vec![search_response("peer", vec![wrong])];

        assert!(rank_candidates(&responses, &ctx, &Quality::Lossless).is_empty());
    }

    #[test]
    fn single_track_selection_tolerates_moderate_duration_drift_on_strong_title() {
        // Megamix segment boundaries and pressing differences shift durations
        // by tens of seconds; an exact title match must survive that.
        let ctx = machine_gun_context(); // expects 245s
        let mut drifted = search_file("Noisia/Split The Atom/01 - Machine Gun.flac", 30_000_000);
        drifted.length = Some(200);
        let responses = vec![search_response("peer", vec![drifted])];

        assert!(!rank_candidates(&responses, &ctx, &Quality::Lossless).is_empty());
    }

    #[test]
    fn single_track_selection_still_rejects_grossly_wrong_duration() {
        // A same-named file nearly twice as long is a different version.
        let ctx = machine_gun_context(); // expects 245s
        let mut extended = search_file("Noisia/Split The Atom/01 - Machine Gun.flac", 60_000_000);
        extended.length = Some(420);
        let responses = vec![search_response("peer", vec![extended])];

        assert!(rank_candidates(&responses, &ctx, &Quality::Lossless).is_empty());
    }

    #[test]
    fn normalize_transliterates_accented_characters() {
        assert_eq!(normalize("Beyoncé"), "beyonce");
        assert_eq!(normalize("Sigur Rós"), "sigur ros");
    }
}
