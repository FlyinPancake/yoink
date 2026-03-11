mod io;
mod lyrics;
mod metadata;
mod worker;

pub(crate) use io::sanitize_path_component;
pub(crate) use metadata::{TrackMetadata, write_audio_metadata};

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::Utc;
use tokio::fs;
use tracing::{debug, error, info, warn};

use uuid::Uuid;

use crate::{
    db,
    error::{AppError, AppResult},
    models::{DownloadJob, DownloadStatus, MonitoredAlbum},
    state::AppState,
    util::is_audio_extension,
};

use super::library::{recompute_partially_wanted, update_wanted};
use io::{parse_track_number_from_path, sanitize_path_component as sanitize};
use metadata::{build_full_artist_string, extract_disc_number};
use worker::download_album_job;

pub(crate) async fn enqueue_album_download(state: &AppState, album: &MonitoredAlbum) {
    // Enqueue if the album is fully wanted (monitored && !acquired)
    // OR partially wanted (has individually monitored tracks not yet acquired).
    let dominated = album.monitored && !album.acquired;
    let partial = album.partially_wanted;
    if !dominated && !partial {
        debug!(
            album_id = %album.id,
            monitored = album.monitored,
            acquired = album.acquired,
            partially_wanted = album.partially_wanted,
            "Skipping enqueue because album is not wanted"
        );
        return;
    }

    let mut jobs = state.download_jobs.write().await;
    if jobs.iter().any(|job| {
        job.album_id == album.id
            && matches!(
                job.status,
                DownloadStatus::Queued | DownloadStatus::Resolving | DownloadStatus::Downloading
            )
    }) {
        debug!(
            album_id = %album.id,
            "Skipping enqueue because active job exists"
        );
        return;
    }

    let requested_quality = album.quality_override.unwrap_or(state.default_quality);

    // Resolve artist name for denormalization
    let artist_name = {
        let artists = state.monitored_artists.read().await;
        artists
            .iter()
            .find(|a| a.id == album.artist_id)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| yoink_shared::UNKNOWN_ARTIST.to_string())
    };

    // Determine the download source from available album provider links
    let source = {
        let links = db::load_album_provider_links(&state.db, album.id)
            .await
            .unwrap_or_default();
        let download_sources = state.registry.download_sources();
        let download_source_ids: Vec<String> = download_sources
            .iter()
            .map(|s| s.id().to_string())
            .collect();

        // Prefer a linked provider that is also a download source.
        let linked = links
            .iter()
            .find(|l| download_source_ids.contains(&l.provider))
            .map(|l| l.provider.clone());

        if let Some(id) = linked {
            id
        } else {
            // Fall back to a source that can operate without provider-linked IDs.
            download_sources
                .iter()
                .find(|s| !s.requires_linked_provider())
                .map(|s| s.id().to_string())
                .or_else(|| download_source_ids.first().cloned())
                .unwrap_or_else(|| "tidal".to_string())
        }
    };

    let mut new_job = DownloadJob {
        id: Uuid::now_v7(),
        album_id: album.id,
        source,
        album_title: album.title.clone(),
        artist_name,
        status: DownloadStatus::Queued,
        quality: requested_quality,
        total_tracks: 0,
        completed_tracks: 0,
        error: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    // Persist to DB
    match db::insert_job(&state.db, &new_job).await {
        Ok(persisted_id) => new_job.id = persisted_id,
        Err(err) => {
            error!(error = %err, "Failed to persist download job to database");
        }
    }

    info!(
        job_id = %new_job.id,
        album_id = %album.id,
        title = %album.title,
        quality = %requested_quality,
        source = %new_job.source,
        "Queued album download"
    );
    jobs.push(new_job);
    drop(jobs);
    state.download_notify.notify_one();
}

pub(crate) async fn download_worker_loop(state: AppState) {
    info!("Download worker started");
    loop {
        let next_job = {
            let mut jobs = state.download_jobs.write().await;
            if let Some(job) = jobs
                .iter_mut()
                .find(|job| job.status == DownloadStatus::Queued)
            {
                job.status = DownloadStatus::Resolving;
                job.updated_at = Utc::now();
                if let Err(e) = db::update_job(&state.db, job).await {
                    warn!(job_id = %job.id, error = %e, "Failed to persist job resolving status");
                }
                Some(job.clone())
            } else {
                None
            }
        };

        let Some(job) = next_job else {
            state.download_notify.notified().await;
            continue;
        };

        info!(
            job_id = %job.id,
            album_id = %job.album_id,
            "Processing download job"
        );

        let outcome = download_album_job(&state, job.clone()).await;
        match outcome {
            Ok(()) => {
                {
                    let mut jobs = state.download_jobs.write().await;
                    if let Some(existing) = jobs.iter_mut().find(|item| item.id == job.id) {
                        existing.status = DownloadStatus::Completed;
                        existing.error = None;
                        existing.updated_at = Utc::now();
                        if let Err(e) = db::update_job(&state.db, existing).await {
                            warn!(job_id = %job.id, error = %e, "Failed to persist job completed status");
                        }
                    }
                }
                info!(
                    job_id = %job.id,
                    album_id = %job.album_id,
                    "Download job completed"
                );
                let mut albums = state.monitored_albums.write().await;
                if let Some(album) = albums.iter_mut().find(|album| album.id == job.album_id) {
                    if album.monitored {
                        // Fully monitored album: all tracks were downloaded
                        album.acquired = true;
                    } else {
                        // Partially wanted album: acquired only if every
                        // monitored track is now acquired.
                        album.acquired = db::all_monitored_tracks_acquired(&state.db, album.id)
                            .await
                            .unwrap_or(false);
                    }
                    update_wanted(album);
                    recompute_partially_wanted(&state.db, album).await;
                    if let Err(e) = db::update_album_flags(
                        &state.db,
                        album.id,
                        album.monitored,
                        album.acquired,
                        album.wanted,
                    )
                    .await
                    {
                        warn!(album_id = %album.id, error = %e, "Failed to persist album flags after download completion");
                    }
                }
                state.notify_sse();
            }
            Err(err) => {
                error!(job_id = %job.id, album_id = %job.album_id, error = %err, "Download job failed");
                {
                    let mut jobs = state.download_jobs.write().await;
                    if let Some(existing) = jobs.iter_mut().find(|item| item.id == job.id) {
                        existing.status = DownloadStatus::Failed;
                        existing.error = Some(err.to_string());
                        existing.updated_at = Utc::now();
                        if let Err(e) = db::update_job(&state.db, existing).await {
                            warn!(job_id = %job.id, error = %e, "Failed to persist job failed status");
                        }
                    }
                }
                let mut albums = state.monitored_albums.write().await;
                if let Some(album) = albums.iter_mut().find(|album| album.id == job.album_id) {
                    album.acquired = false;
                    update_wanted(album);
                    recompute_partially_wanted(&state.db, album).await;
                    if let Err(e) = db::update_album_flags(
                        &state.db,
                        album.id,
                        album.monitored,
                        album.acquired,
                        album.wanted,
                    )
                    .await
                    {
                        warn!(album_id = %album.id, error = %e, "Failed to persist album flags after download failure");
                    }
                }
                state.notify_sse();
            }
        }
    }
}

pub(crate) async fn retag_existing_files(state: &AppState) -> AppResult<(usize, usize, usize)> {
    let artists = state.monitored_artists.read().await.clone();
    let albums = state.monitored_albums.read().await.clone();

    let artist_names: HashMap<Uuid, String> = artists.into_iter().map(|a| (a.id, a.name)).collect();

    let mut tagged_files = 0usize;
    let mut missing_files = 0usize;
    let mut scanned_albums = 0usize;

    for album in albums.into_iter().filter(|a| a.acquired) {
        let Some(artist_name) = artist_names.get(&album.artist_id) else {
            continue;
        };

        // Find a metadata provider link for this album
        let album_links = db::load_album_provider_links(&state.db, album.id)
            .await
            .unwrap_or_default();

        // Find the first link that has a matching metadata provider
        let provider_link = album_links
            .iter()
            .find(|l| state.registry.metadata_provider(&l.provider).is_some());
        let Some(link) = provider_link else {
            continue;
        };

        let provider = state.registry.metadata_provider(&link.provider).unwrap();

        let release_suffix = album
            .release_date
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());

        // Fetch cover art via provider
        let cover_art = if let Some(cover_ref) = link.cover_ref.as_deref() {
            provider.fetch_cover_art_bytes(cover_ref).await
        } else {
            fetch_cover_art_bytes_from_url(&state.http, album.cover_url.as_deref()).await
        };

        let album_dir = state
            .music_root
            .join(sanitize(artist_name))
            .join(sanitize(&format!("{} ({})", album.title, release_suffix)));

        if !fs::try_exists(&album_dir).await.unwrap_or(false) {
            continue;
        }

        scanned_albums += 1;

        let (provider_tracks, album_extra) = match provider.fetch_tracks(&link.external_id).await {
            Ok(result) => result,
            Err(err) => {
                info!(
                    album_id = %album.id,
                    provider = %link.provider,
                    error = %err,
                    "Failed to fetch tracks for retagging"
                );
                continue;
            }
        };

        let total_tracks = provider_tracks.len() as u32;

        let mut files_by_track: HashMap<u32, PathBuf> = HashMap::new();
        let mut ordered_files: Vec<PathBuf> = Vec::new();

        let mut entries = fs::read_dir(&album_dir).await.map_err(|err| {
            AppError::filesystem("read album directory", album_dir.display().to_string(), err)
        })?;
        while let Some(entry) = entries.next_entry().await.map_err(|err| {
            AppError::filesystem("read directory entry", album_dir.display().to_string(), err)
        })? {
            let path = entry.path();
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !is_audio_extension(ext) {
                continue;
            }

            if let Some(track_number) = parse_track_number_from_path(&path) {
                files_by_track.insert(track_number, path.clone());
            }
            ordered_files.push(path);
        }

        ordered_files.sort();

        for (idx, track) in provider_tracks.iter().enumerate() {
            let track_number = track.track_number;
            let track_info_extra = provider.fetch_track_info_extra(&track.external_id).await;
            let track_artist = build_full_artist_string(
                &track.title,
                &track.extra,
                track_info_extra.as_ref(),
                artist_name,
            );
            let disc_number = extract_disc_number(&track.extra, track_info_extra.as_ref());
            let path = files_by_track
                .get(&track_number)
                .cloned()
                .or_else(|| ordered_files.get(idx).cloned());

            let Some(path) = path else {
                missing_files += 1;
                continue;
            };

            write_audio_metadata(&TrackMetadata {
                path: &path,
                title: &track.title,
                track_artist: &track_artist,
                album_artist: artist_name,
                album: &album.title,
                track_number,
                disc_number,
                total_tracks,
                release_date: &release_suffix,
                track_extra: &track.extra,
                album_extra: &album_extra,
                track_info_extra: track_info_extra.as_ref(),
                lyrics_text: None,
                cover_art_jpeg: cover_art.as_deref(),
            })?;
            tagged_files += 1;
        }
    }

    Ok((tagged_files, missing_files, scanned_albums))
}

pub(crate) async fn remove_downloaded_album_files(
    state: &AppState,
    album: &MonitoredAlbum,
) -> AppResult<bool> {
    let artist_name = {
        let artists = state.monitored_artists.read().await;
        artists
            .iter()
            .find(|artist| artist.id == album.artist_id)
            .map(|artist| artist.name.clone())
            .unwrap_or_else(|| yoink_shared::UNKNOWN_ARTIST.to_string())
    };

    let release_suffix = album
        .release_date
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());

    let artist_dir = state.music_root.join(sanitize(&artist_name));
    let album_dir = artist_dir.join(sanitize(&format!("{} ({})", album.title, release_suffix)));

    let exists = fs::try_exists(&album_dir).await.map_err(|err| {
        AppError::filesystem(
            "check album directory",
            album_dir.display().to_string(),
            err,
        )
    })?;
    if !exists {
        return Ok(false);
    }

    fs::remove_dir_all(&album_dir).await.map_err(|err| {
        AppError::filesystem(
            "remove album directory",
            album_dir.display().to_string(),
            err,
        )
    })?;

    if let Ok(mut entries) = fs::read_dir(&artist_dir).await {
        let is_empty = entries.next_entry().await.ok().flatten().is_none();
        if is_empty {
            let _ = fs::remove_dir(&artist_dir).await;
        }
    }

    Ok(true)
}

/// Fetch cover art from an already-resolved URL (or a provider image proxy URL).
pub(super) async fn fetch_cover_art_bytes_from_url(
    http: &reqwest::Client,
    cover_url: Option<&str>,
) -> Option<Vec<u8>> {
    let url = cover_url?;
    if url.starts_with('/') {
        if let Some(resolved_url) = resolve_cover_art_url(url) {
            let resp = http.get(&resolved_url).send().await.ok()?;
            if resp.status().is_success() {
                return resp.bytes().await.ok().map(|b| b.to_vec());
            }
        }
        return None;
    }
    let resp = http.get(url).send().await.ok()?;
    if resp.status().is_success() {
        resp.bytes().await.ok().map(|b| b.to_vec())
    } else {
        None
    }
}

fn resolve_cover_art_url(url: &str) -> Option<String> {
    if !url.starts_with("/api/image/") {
        return None;
    }

    let parts: Vec<&str> = url.split('/').filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        ["api", "image", image_ref, size] => {
            let size = size.parse::<u16>().ok()?;
            resolve_provider_cover_url("tidal", image_ref, size)
        }
        ["api", "image", provider, image_ref, size] => {
            let size = size.parse::<u16>().ok()?;
            resolve_provider_cover_url(provider, image_ref, size)
        }
        _ => None,
    }
}

fn resolve_provider_cover_url(provider: &str, image_ref: &str, size: u16) -> Option<String> {
    match provider {
        "tidal" => Some(format!(
            "https://resources.tidal.com/images/{}/{size}x{size}.jpg",
            image_ref.replace('-', "/")
        )),
        "deezer" => {
            let size = if size <= 56 {
                56
            } else if size <= 250 {
                250
            } else if size <= 500 {
                500
            } else {
                1000
            };
            let (image_type, md5) = if let Some(stripped) = image_ref.strip_prefix("artist:") {
                ("artist", stripped)
            } else {
                ("cover", image_ref)
            };
            Some(format!(
                "https://cdn-images.dzcdn.net/images/{image_type}/{md5}/{size}x{size}-000000-80-0-0.jpg"
            ))
        }
        "musicbrainz" => {
            let size = if size <= 250 {
                250
            } else if size <= 500 {
                500
            } else {
                1200
            };
            Some(format!(
                "https://coverartarchive.org/release-group/{image_ref}/front-{size}"
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::models::DownloadStatus;
    use crate::test_helpers::*;
    use yoink_shared::Quality;

    #[tokio::test]
    async fn enqueue_album_download_uses_album_quality_override() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let mut album = seed_album(&state.db, artist.id, "Album").await;
        album.quality_override = Some(Quality::HiRes);

        state.monitored_artists.write().await.push(artist);
        state.monitored_albums.write().await.push(album.clone());

        super::enqueue_album_download(&state, &album).await;

        let jobs = state.download_jobs.read().await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, DownloadStatus::Queued);
        assert_eq!(jobs[0].quality, Quality::HiRes);
    }

    #[test]
    fn resolve_cover_art_url_supports_deezer_proxy_urls() {
        let resolved =
            super::resolve_cover_art_url("/api/image/deezer/a8e283061c74e47494b1968a40e63943/640");
        assert_eq!(
            resolved.as_deref(),
            Some(
                "https://cdn-images.dzcdn.net/images/cover/a8e283061c74e47494b1968a40e63943/1000x1000-000000-80-0-0.jpg"
            )
        );
    }

    #[test]
    fn resolve_cover_art_url_supports_tidal_legacy_proxy_urls() {
        let resolved =
            super::resolve_cover_art_url("/api/image/12345678-1234-1234-1234-123456789abc/640");
        assert_eq!(
            resolved.as_deref(),
            Some(
                "https://resources.tidal.com/images/12345678/1234/1234/1234/123456789abc/640x640.jpg"
            )
        );
    }
}
