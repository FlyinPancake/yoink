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
    matching::{pick_best_candidate, pick_from_album_bundle},
    models::*,
    transfer::{is_complete_success, is_failure},
    util::{dedup_queries, normalize, percent_encode_path, sanitize_relative_path},
};
use super::{DownloadTrackContext, PlaybackInfo, ProviderError, SearchTrackResolver};

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
        let candidate = match self.find_album_bundle_candidate(ctx, quality).await? {
            Some(c) => c,
            None => self.find_single_track_candidate(ctx, quality).await?,
        };

        self.enqueue_download(&candidate.username, &candidate.filename, candidate.size)
            .await?;

        let local_path = match self
            .wait_for_download(&candidate.username, &candidate.filename, 180)
            .await
        {
            Ok(path) => path,
            Err(e) => {
                warn!(
                    username = candidate.username,
                    filename = candidate.filename,
                    error = %e,
                    "SoulSeek transfer did not complete in time"
                );
                return Err(e);
            }
        };

        Ok(PlaybackInfo::LocalFile(local_path))
    }
}

// ── High-level search strategies ────────────────────────────────────

impl SoulSeekSource {
    async fn find_album_bundle_candidate(
        &self,
        ctx: &DownloadTrackContext,
        quality: &Quality,
    ) -> Result<Option<matching::Candidate>, ProviderError> {
        let responses = self.search_album_queries(ctx, quality).await?;
        if responses.is_empty() {
            return Ok(None);
        }
        Ok(pick_from_album_bundle(&responses, ctx, quality))
    }

    async fn find_single_track_candidate(
        &self,
        ctx: &DownloadTrackContext,
        quality: &Quality,
    ) -> Result<matching::Candidate, ProviderError> {
        let responses = self.search_track_queries(ctx).await?;

        ensure!(
            !responses.is_empty(),
            NotFoundSnafu {
                provider: Provider::Soulseek,
                resource: format!("search responses for track '{}'", ctx.track_title),
            }
        );

        pick_best_candidate(&responses, ctx, quality).context(NotFoundSnafu {
            provider: Provider::Soulseek,
            resource: format!("suitable candidate for '{}'", ctx.track_title),
        })
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
    async fn enqueue_download(
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
        let responses = vec![SearchResponse {
            username: "user1".to_string(),
            files: vec![
                search_file("The Artist/The Album/01 - Song One.flac", 100),
                search_file("The Artist/The Album/02 - Song Two.flac", 100),
            ],
        }];

        let candidate = pick_from_album_bundle(&responses, &ctx, &Quality::Lossless);
        assert!(candidate.is_none());
    }

    #[test]
    fn album_bundle_selection_picks_requested_track_from_complete_bundle() {
        let ctx = test_context("Song Two", 2, 2);
        let responses = vec![SearchResponse {
            username: "user1".to_string(),
            files: vec![
                search_file("The Artist/The Album/01 - Song One.flac", 100),
                search_file("The Artist/The Album/02 - Song Two.flac", 100),
            ],
        }];

        let candidate = pick_from_album_bundle(&responses, &ctx, &Quality::Lossless)
            .expect("expected complete album candidate");
        assert_eq!(candidate.username, "user1");
        assert!(candidate.filename.contains("02 - Song Two"));
    }

    #[test]
    fn album_bundle_selection_rejects_track_number_with_wrong_title() {
        let ctx = test_context("Song One", 2, 2);
        let responses = vec![SearchResponse {
            username: "user1".to_string(),
            files: vec![
                search_file("The Artist/The Album/01 - Song One.flac", 100),
                search_file("The Artist/The Album/02 - Interlude.flac", 100),
            ],
        }];

        let candidate = pick_from_album_bundle(&responses, &ctx, &Quality::Lossless);
        assert!(candidate.is_none());
    }

    #[test]
    fn single_track_selection_ignores_non_audio_files() {
        let ctx = test_context("Song One", 1, 1);
        let responses = vec![SearchResponse {
            username: "user1".to_string(),
            files: vec![search_file("The Artist/The Album/Song One.jpg", 100)],
        }];

        assert!(pick_best_candidate(&responses, &ctx, &Quality::Lossless).is_none());
    }

    #[test]
    fn single_track_selection_detects_extension_from_filename() {
        let ctx = test_context("Song One", 1, 1);
        let mut file = search_file("The Artist/The Album/Song One.FLAC", 100);
        file.extension = None;
        let responses = vec![SearchResponse {
            username: "user1".to_string(),
            files: vec![file],
        }];

        let candidate = pick_best_candidate(&responses, &ctx, &Quality::Lossless)
            .expect("expected audio candidate");
        assert!(candidate.filename.ends_with("Song One.FLAC"));
    }

    #[test]
    fn single_track_selection_rejects_title_substring_from_different_song() {
        let ctx = DownloadTrackContext {
            artist_name: "Noisia".to_string(),
            album_title: "Split The Atom".to_string(),
            track_title: "Machine Gun".to_string(),
            track_number: Some(1),
            album_track_count: Some(19),
            duration_secs: Some(245),
        };
        let mut wrong = search_file(
            "shared/_Untagged/Klute_Unknown Artist/_Unknown Album/03 - Machine Gun Etiquette.flac",
            50_730_582,
        );
        wrong.length = Some(444);
        let responses = vec![SearchResponse {
            username: "peer".to_string(),
            files: vec![wrong],
        }];

        assert!(pick_best_candidate(&responses, &ctx, &Quality::Lossless).is_none());
    }

    #[test]
    fn single_track_selection_accepts_compatible_title_and_duration() {
        let ctx = DownloadTrackContext {
            artist_name: "Noisia".to_string(),
            album_title: "Split The Atom".to_string(),
            track_title: "Machine Gun".to_string(),
            track_number: Some(1),
            album_track_count: Some(19),
            duration_secs: Some(245),
        };
        let mut correct = search_file(
            "Noisia/Split The Atom/01 - Noisia - Machine Gun (Original Mix).flac",
            30_000_000,
        );
        correct.length = Some(245);
        let responses = vec![SearchResponse {
            username: "peer".to_string(),
            files: vec![correct],
        }];

        let candidate = pick_best_candidate(&responses, &ctx, &Quality::Lossless)
            .expect("expected compatible candidate");
        assert!(candidate.filename.contains("Machine Gun (Original Mix)"));
    }
}
