use std::sync::Arc;

use chrono::Utc;
use tokio::fs;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};
use yoink_shared::Quality;

use crate::{
    db,
    error::{AppError, AppResult},
    models::{DownloadJob, DownloadStatus},
    providers::{
        DownloadSource, DownloadTrackContext, MetadataProvider, PlaybackInfo, ProviderTrack,
    },
    state::AppState,
    util::provider_priority,
};

use super::fetch_cover_art_bytes_from_url;
use super::io::{has_flac_stream_marker, sanitize_path_component, sniff_media_container};
use super::lyrics::{fetch_track_lyrics, write_lrc_sidecar};
use super::metadata::{
    TrackMetadata, build_full_artist_string, extract_disc_number, write_audio_metadata,
};

#[derive(Clone)]
struct PlannedTrackDownload {
    provider_track: ProviderTrack,
    existing_track: Option<yoink_shared::TrackInfo>,
}

pub(crate) async fn download_album_job(state: &AppState, job: DownloadJob) -> AppResult<()> {
    let requested_quality = job.quality;

    // Resolve the provider link for this album to find the external ID and provider
    let album_links = db::load_album_provider_links(&state.db, job.album_id).await?;

    let download_source = state.registry.download_source(&job.source).ok_or_else(|| {
        AppError::unavailable("download source", format!("'{}' not available", job.source))
    })?;

    // Pick a metadata provider link independently from the download source.
    // If the source itself has metadata, prefer it; otherwise use highest-priority linked metadata.
    let metadata_link = album_links
        .iter()
        .find(|l| {
            l.provider == job.source && state.registry.metadata_provider(&l.provider).is_some()
        })
        .or_else(|| {
            album_links
                .iter()
                .filter(|l| state.registry.metadata_provider(&l.provider).is_some())
                .max_by_key(|l| provider_priority(&l.provider))
        })
        .ok_or_else(|| {
            AppError::not_found("metadata provider link", Some(job.album_id.to_string()))
        })?;

    let metadata_provider = state
        .registry
        .metadata_provider(&metadata_link.provider)
        .ok_or_else(|| {
            AppError::unavailable(
                "metadata provider",
                format!("'{}' not available", metadata_link.provider),
            )
        })?;

    let external_album_id = &metadata_link.external_id;

    info!(
        job_id = %job.id,
        album_id = %job.album_id,
        external_album_id = %external_album_id,
        source = %job.source,
        artist_name = %job.artist_name,
        quality = %requested_quality,
        "Starting album download"
    );

    let artist_name = &job.artist_name;

    // Fetch tracks from the metadata provider
    let (provider_tracks, album_extra) = metadata_provider.fetch_tracks(external_album_id).await?;

    if provider_tracks.is_empty() {
        return Err(AppError::not_found(
            "downloadable tracks",
            Some(job.album_id.to_string()),
        ));
    }

    // Save the full album track count for metadata tagging (track N of M).
    let full_album_track_count = provider_tracks.len();

    // When the album is not fully monitored (partially_wanted), filter to only
    // the individually monitored tracks. Matching is done by ISRC or
    // (disc, track_number) position.
    let album_is_fully_monitored = {
        let albums = state.monitored_albums.read().await;
        albums
            .iter()
            .find(|a| a.id == job.album_id)
            .map(|a| a.monitored)
            .unwrap_or(false)
    };

    let local_tracks = db::load_tracks_for_album(&state.db, job.album_id)
        .await
        .unwrap_or_default();

    let mut planned_tracks = Vec::with_capacity(provider_tracks.len());
    for provider_track in provider_tracks {
        let existing_track = match_local_track(
            state,
            &metadata_link.provider,
            &provider_track,
            &local_tracks,
        )
        .await;

        planned_tracks.push(PlannedTrackDownload {
            provider_track,
            existing_track,
        });
    }

    let planned_tracks = if album_is_fully_monitored {
        planned_tracks
    } else {
        let monitored_tracks: Vec<PlannedTrackDownload> = planned_tracks
            .into_iter()
            .filter(|planned| {
                planned
                    .existing_track
                    .as_ref()
                    .map(|track| track.monitored)
                    .unwrap_or(false)
            })
            .collect();

        if monitored_tracks.is_empty() {
            return Err(AppError::not_found(
                "monitored tracks",
                Some(job.album_id.to_string()),
            ));
        }

        monitored_tracks
    };

    if planned_tracks.is_empty() {
        return Err(AppError::not_found(
            "matching provider tracks",
            Some(job.album_id.to_string()),
        ));
    }

    let total_tracks = planned_tracks.len();
    update_job_progress(
        state,
        job.id,
        total_tracks,
        0,
        DownloadStatus::Downloading,
        None,
    )
    .await;

    let album = {
        let albums = state.monitored_albums.read().await;
        albums
            .iter()
            .find(|album| album.id == job.album_id)
            .cloned()
    };
    let release_suffix = album
        .as_ref()
        .and_then(|album| album.release_date.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let cover_art = fetch_album_cover_art(
        &state.http,
        metadata_provider.as_ref(),
        metadata_link,
        album.as_ref(),
    )
    .await;

    let artist_dir = state.music_root.join(sanitize_path_component(artist_name));
    let album_dir = artist_dir.join(sanitize_path_component(&format!(
        "{} ({})",
        job.album_title, release_suffix
    )));
    fs::create_dir_all(&album_dir).await.map_err(|err| {
        AppError::filesystem(
            "create output directory",
            album_dir.display().to_string(),
            err,
        )
    })?;

    let max_parallel = state.download_max_parallel_tracks.max(1);
    info!(
        job_id = %job.id,
        album_id = %job.album_id,
        max_parallel_tracks = max_parallel,
        "Downloading tracks with configured parallelism"
    );

    let mut completed_tracks = 0usize;
    let mut join_set = JoinSet::new();
    let mut track_iter = planned_tracks.into_iter();
    let mut in_flight = 0usize;

    while in_flight < max_parallel {
        let Some(track) = track_iter.next() else {
            break;
        };
        in_flight += 1;

        let state_clone = state.clone();
        let job_clone = job.clone();
        let requested_quality_clone = requested_quality;
        let download_source_clone = Arc::clone(&download_source);
        let metadata_provider_clone = Arc::clone(&metadata_provider);
        let metadata_provider_id = metadata_provider.id().to_string();
        let artist_name_owned = artist_name.to_string();
        let release_suffix_owned = release_suffix.clone();
        let album_dir_clone = album_dir.clone();
        let album_extra_clone = album_extra.clone();
        let cover_art_clone = cover_art.clone();

        let full_count = full_album_track_count;
        join_set.spawn(async move {
            process_track_download(
                state_clone,
                job_clone,
                track,
                requested_quality_clone,
                download_source_clone,
                metadata_provider_clone,
                metadata_provider_id,
                artist_name_owned,
                release_suffix_owned,
                album_dir_clone,
                album_extra_clone,
                cover_art_clone,
                album_is_fully_monitored,
                total_tracks,
                full_count,
            )
            .await
        });
    }

    while in_flight > 0 {
        let Some(result) = join_set.join_next().await else {
            break;
        };
        in_flight -= 1;

        match result {
            Ok(Ok(())) => {
                completed_tracks += 1;
                update_job_progress(
                    state,
                    job.id,
                    total_tracks,
                    completed_tracks,
                    DownloadStatus::Downloading,
                    None,
                )
                .await;

                if let Some(next_track) = track_iter.next() {
                    in_flight += 1;

                    let state_clone = state.clone();
                    let job_clone = job.clone();
                    let requested_quality_clone = requested_quality;
                    let download_source_clone = Arc::clone(&download_source);
                    let metadata_provider_clone = Arc::clone(&metadata_provider);
                    let metadata_provider_id = metadata_provider.id().to_string();
                    let artist_name_owned = artist_name.to_string();
                    let release_suffix_owned = release_suffix.clone();
                    let album_dir_clone = album_dir.clone();
                    let album_extra_clone = album_extra.clone();
                    let cover_art_clone = cover_art.clone();
                    let full_count = full_album_track_count;

                    join_set.spawn(async move {
                        process_track_download(
                            state_clone,
                            job_clone,
                            next_track,
                            requested_quality_clone,
                            download_source_clone,
                            metadata_provider_clone,
                            metadata_provider_id,
                            artist_name_owned,
                            release_suffix_owned,
                            album_dir_clone,
                            album_extra_clone,
                            cover_art_clone,
                            album_is_fully_monitored,
                            total_tracks,
                            full_count,
                        )
                        .await
                    });
                }
            }
            Ok(Err(err)) => return Err(err),
            Err(err) => {
                return Err(AppError::task_join(err.to_string()));
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn process_track_download(
    state: AppState,
    job: DownloadJob,
    planned_track: PlannedTrackDownload,
    requested_quality: Quality,
    download_source: Arc<dyn DownloadSource>,
    metadata_provider: Arc<dyn MetadataProvider>,
    metadata_provider_id: String,
    artist_name: String,
    release_suffix: String,
    album_dir: std::path::PathBuf,
    album_extra: std::collections::HashMap<String, serde_json::Value>,
    cover_art: Option<Vec<u8>>,
    album_is_fully_monitored: bool,
    _total_tracks: usize,
    // Full album track count for metadata tagging (track N of M).
    // May differ from the download count when only a subset is being downloaded.
    full_album_track_count: usize,
) -> AppResult<()> {
    let track = planned_track.provider_track;
    let existing_track = planned_track.existing_track;
    let effective_quality = existing_track
        .as_ref()
        .and_then(|track| track.quality_override)
        .unwrap_or(requested_quality);
    let base_name = format!(
        "{:02} - {}",
        track.track_number,
        sanitize_path_component(&track.title)
    );

    if let Some(existing_path) = find_existing_track_file(&album_dir, &base_name).await {
        info!(
            track_id = %track.external_id,
            file = %existing_path.display(),
            "Skipping already downloaded track"
        );
        return Ok(());
    }

    debug!(
        job_id = %job.id,
        album_id = %job.album_id,
        track_id = %track.external_id,
        track_number = track.track_number,
        track_title = %track.title,
        "Resolving track playback"
    );

    let track_context = DownloadTrackContext {
        artist_name: artist_name.clone(),
        album_title: job.album_title.clone(),
        track_title: track.title.clone(),
        track_number: Some(track.track_number),
        album_track_count: Some(full_album_track_count),
        duration_secs: Some(track.duration_secs),
    };

    let (track_payload, resolved_source_id) = resolve_playback_with_fallback(
        &state,
        download_source.as_ref(),
        &metadata_provider_id,
        &track.external_id,
        &track.title,
        &effective_quality,
        &track_context,
    )
    .await?;

    if resolved_source_id != download_source.id() {
        info!(
            requested_source = %download_source.id(),
            resolved_source = %resolved_source_id,
            track_id = %track.external_id,
            track_title = %track.title,
            "Resolved playback via fallback source"
        );
    }

    if matches!(track_payload, PlaybackInfo::SegmentUrls(_)) {
        info!(
            track_id = %track.external_id,
            album_id = %job.album_id,
            "Using segment download for track"
        );
    }

    let track_number = track.track_number;
    let track_info_extra = metadata_provider
        .fetch_track_info_extra(&track.external_id)
        .await;
    let track_artist = build_full_artist_string(
        &track.title,
        &track.extra,
        track_info_extra.as_ref(),
        &artist_name,
    );
    let disc_number = extract_disc_number(&track.extra, track_info_extra.as_ref());
    let lyrics = if state.download_lyrics {
        fetch_track_lyrics(
            &state,
            &track.title,
            &artist_name,
            &job.album_title,
            Some(track.duration_secs),
        )
        .await
    } else {
        None
    };

    let temp_path = album_dir.join(format!("{base_name}.part"));

    download_playback_to_file(&state.http, &track_payload, &temp_path)
        .await
        .map_err(|err| AppError::download("download track", format!("{}: {err}", track.title)))?;

    let mut final_ext = sniff_final_extension(&temp_path, effective_quality).await;
    if effective_quality == Quality::HiRes {
        let is_flac = has_flac_stream_marker(&temp_path).await.map_err(|err| {
            AppError::download("validate track format", format!("{}: {err}", track.title))
        })?;
        if !is_flac {
            let container = sniff_media_container(&temp_path)
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            if container == "mp4" {
                final_ext = "m4a";
                info!(
                    track_id = %track.external_id,
                    album_id = %job.album_id,
                    file = %temp_path.display(),
                    "HI_RES track is MP4 container with FLAC audio; keeping as .m4a"
                );
            } else {
                warn!(
                    track_id = %track.external_id,
                    album_id = %job.album_id,
                    file = %temp_path.display(),
                    container = %container,
                    "HI_RES output is not FLAC, retrying track in LOSSLESS"
                );

                let (lossless_payload, _) = resolve_playback_with_fallback(
                    &state,
                    download_source.as_ref(),
                    &metadata_provider_id,
                    &track.external_id,
                    &track.title,
                    &Quality::Lossless,
                    &track_context,
                )
                .await
                .map_err(|e| {
                    AppError::download(
                        "resolve lossless fallback",
                        format!(
                            "Failed to resolve LOSSLESS playback for {}: {e}",
                            track.title
                        ),
                    )
                })?;

                download_playback_to_file(&state.http, &lossless_payload, &temp_path)
                    .await
                    .map_err(|err| {
                        AppError::download(
                            "download lossless fallback",
                            format!("failed track {} in LOSSLESS fallback: {err}", track.title),
                        )
                    })?;

                let fallback_is_flac = has_flac_stream_marker(&temp_path).await.map_err(|err| {
                    AppError::download(
                        "validate lossless fallback",
                        format!(
                            "failed validating LOSSLESS fallback track {}: {err}",
                            track.title
                        ),
                    )
                })?;
                if !fallback_is_flac {
                    return Err(AppError::validation(
                        Some("audio_format"),
                        format!(
                            "track {} is not FLAC even after LOSSLESS fallback",
                            track.title
                        ),
                    ));
                }
            }
        }
    }

    let final_path = album_dir.join(format!("{base_name}.{final_ext}"));
    fs::rename(&temp_path, &final_path).await.map_err(|err| {
        AppError::filesystem("finalize track file", final_path.display().to_string(), err)
    })?;

    if let Err(err) = write_audio_metadata(&TrackMetadata {
        path: &final_path,
        title: &track.title,
        track_artist: &track_artist,
        album_artist: &artist_name,
        album: &job.album_title,
        track_number,
        disc_number,
        total_tracks: full_album_track_count as u32,
        release_date: &release_suffix,
        track_extra: &track.extra,
        album_extra: &album_extra,
        track_info_extra: track_info_extra.as_ref(),
        lyrics_text: lyrics.as_ref().and_then(|v| v.embedded_text.as_deref()),
        cover_art_jpeg: cover_art.as_deref(),
    }) {
        warn!(
            track_id = %track.external_id,
            file = %final_path.display(),
            error = %err,
            "Skipping metadata write for track"
        );
    }

    if let Some(synced) = lyrics.as_ref().and_then(|v| v.synced_lrc.as_deref())
        && let Err(err) = write_lrc_sidecar(&final_path, synced).await
    {
        warn!(
            track_id = %track.external_id,
            file = %final_path.display(),
            error = %err,
            "Skipping LRC sidecar write"
        );
    }

    // Persist track with artist and file path info
    let relative_path = final_path
        .strip_prefix(&state.music_root)
        .unwrap_or(&final_path)
        .to_string_lossy()
        .to_string();
    let local_track_id = existing_track
        .as_ref()
        .map(|t| t.id)
        .unwrap_or_else(uuid::Uuid::now_v7);

    let track_info = track.to_track_info(crate::providers::LocalTrackOverrides {
        id: local_track_id,
        quality_override: existing_track.as_ref().and_then(|t| t.quality_override),
        track_artist: Some(track_artist.clone()),
        file_path: Some(relative_path),
        monitored: existing_track
            .as_ref()
            .map(|t| t.monitored)
            .unwrap_or(album_is_fully_monitored),
        acquired: true, // Just downloaded — mark as acquired
        disc_number: Some(disc_number.unwrap_or(1)),
        track_number: Some(track_number),
        explicit: Some(track.explicit),
        ..Default::default()
    });
    if let Err(e) = db::upsert_track(&state.db, &track_info, job.album_id).await {
        warn!(track_id = %local_track_id, error = %e, "Failed to persist downloaded track to DB");
    }
    if let Err(e) = db::upsert_track_provider_link(
        &state.db,
        local_track_id,
        &metadata_provider_id,
        &track.external_id,
    )
    .await
    {
        warn!(track_id = %local_track_id, error = %e, "Failed to persist track provider link");
    }

    Ok(())
}

async fn find_existing_track_file(
    album_dir: &std::path::Path,
    base_name: &str,
) -> Option<std::path::PathBuf> {
    for ext in crate::util::AUDIO_EXTENSIONS {
        let path = album_dir.join(format!("{base_name}.{ext}"));
        if tokio::fs::try_exists(&path).await.ok()? {
            return Some(path);
        }
    }
    None
}

async fn sniff_final_extension(path: &std::path::Path, quality: Quality) -> &'static str {
    match sniff_media_container(path).await.as_deref() {
        Ok("flac") => "flac",
        Ok("mp4") => "m4a",
        Ok("mp3") => "mp3",
        Ok("ogg") => "ogg",
        Ok("wav") => "wav",
        Ok("aac") => "aac",
        _ => match quality {
            Quality::HiRes | Quality::Lossless => "flac",
            Quality::High | Quality::Low => "mp3",
        },
    }
}

async fn match_local_track(
    state: &AppState,
    metadata_provider_id: &str,
    provider_track: &ProviderTrack,
    local_tracks: &[yoink_shared::TrackInfo],
) -> Option<yoink_shared::TrackInfo> {
    if let Ok(Some(track_id)) = db::find_track_by_provider_link(
        &state.db,
        metadata_provider_id,
        &provider_track.external_id,
    )
    .await
        && let Some(track) = local_tracks.iter().find(|track| track.id == track_id)
    {
        return Some(track.clone());
    }

    if let Some(isrc) = provider_track.isrc.as_deref()
        && let Some(track) = local_tracks.iter().find(|track| {
            track
                .isrc
                .as_deref()
                .map(|candidate| candidate.eq_ignore_ascii_case(isrc))
                .unwrap_or(false)
        })
    {
        return Some(track.clone());
    }

    let disc_number = extract_disc_number(&provider_track.extra, None).unwrap_or(1);
    local_tracks
        .iter()
        .find(|track| {
            track.disc_number == disc_number && track.track_number == provider_track.track_number
        })
        .cloned()
}

async fn fetch_album_cover_art(
    http: &reqwest::Client,
    metadata_provider: &dyn MetadataProvider,
    metadata_link: &db::AlbumProviderLink,
    album: Option<&yoink_shared::MonitoredAlbum>,
) -> Option<Vec<u8>> {
    if let Some(cover_ref) = metadata_link.cover_ref.as_deref()
        && let Some(bytes) = metadata_provider.fetch_cover_art_bytes(cover_ref).await
    {
        return Some(bytes);
    }

    fetch_cover_art_bytes_from_url(http, album.and_then(|album| album.cover_url.as_deref())).await
}

pub(crate) async fn update_job_progress(
    state: &AppState,
    job_id: uuid::Uuid,
    total_tracks: usize,
    completed_tracks: usize,
    status: DownloadStatus,
    error: Option<String>,
) {
    let mut jobs = state.download_jobs.write().await;
    if let Some(job) = jobs.iter_mut().find(|job| job.id == job_id) {
        job.total_tracks = total_tracks;
        job.completed_tracks = completed_tracks;
        job.status = status;
        job.error = error;
        job.updated_at = Utc::now();
        if let Err(e) = db::update_job(&state.db, job).await {
            warn!(job_id = %job_id, error = %e, "Failed to persist job progress to DB");
        }
        debug!(
            job_id = %job_id,
            album_id = %job.album_id,
            status = job.status.as_str(),
            completed_tracks = job.completed_tracks,
            total_tracks = job.total_tracks,
            "Updated download progress"
        );
        state.notify_sse();
    }
}

/// Download a PlaybackInfo payload to a file using the io module.
async fn download_playback_to_file(
    http: &reqwest::Client,
    payload: &PlaybackInfo,
    path: &std::path::Path,
) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            AppError::filesystem("create parent directory", parent.display().to_string(), e)
        })?;
    }

    use super::io::DownloadPayload;
    let io_payload = match payload {
        PlaybackInfo::DirectUrl(url) => DownloadPayload::DirectUrl(url.clone()),
        PlaybackInfo::SegmentUrls(urls) => DownloadPayload::DashSegmentUrls(urls.clone()),
        PlaybackInfo::LocalFile(local_path) => {
            tokio::fs::copy(local_path, path).await.map_err(|e| {
                AppError::filesystem("copy local file", local_path.display().to_string(), e)
            })?;
            return Ok(());
        }
    };
    super::io::download_payload_to_file(http, &io_payload, path).await
}

async fn resolve_playback_with_fallback(
    state: &AppState,
    primary_source: &dyn crate::providers::DownloadSource,
    metadata_provider_id: &str,
    external_track_id: &str,
    track_title: &str,
    quality: &Quality,
    context: &DownloadTrackContext,
) -> AppResult<(PlaybackInfo, String)> {
    match primary_source
        .resolve_playback(external_track_id, quality, Some(context))
        .await
    {
        Ok(payload) => return Ok((payload, primary_source.id().to_string())),
        Err(primary_err) => {
            warn!(
                source = %primary_source.id(),
                track_id = %external_track_id,
                track_title,
                error = %primary_err,
                "Primary download source failed to resolve playback; attempting fallback"
            );
        }
    }

    for source in state.registry.download_sources() {
        if source.id() == primary_source.id() {
            continue;
        }

        // Only try sources that can work without linked provider IDs,
        // or those that match the metadata provider we fetched tracks from.
        if source.requires_linked_provider() && source.id() != metadata_provider_id {
            continue;
        }

        match source
            .resolve_playback(external_track_id, quality, Some(context))
            .await
        {
            Ok(payload) => return Ok((payload, source.id().to_string())),
            Err(err) => {
                warn!(
                    source = %source.id(),
                    track_id = %external_track_id,
                    track_title,
                    error = %err,
                    "Fallback download source failed to resolve playback"
                );
            }
        }
    }

    Err(AppError::unavailable(
        "playback",
        format!("failed to resolve playback for {track_title}: no source could resolve track"),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use axum::{Router, routing::get};
    use tokio::fs;
    use tokio::net::TcpListener;

    use crate::db;
    use crate::db::AlbumProviderLink;
    use crate::providers::registry::ProviderRegistry;
    use crate::providers::{PlaybackInfo, ProviderTrack};
    use crate::test_helpers::*;
    use yoink_shared::{MonitoredAlbum, Quality};

    fn provider_track(external_id: &str, track_number: u32) -> ProviderTrack {
        ProviderTrack {
            external_id: external_id.to_string(),
            title: format!("Track {track_number}"),
            version: None,
            track_number,
            disc_number: Some(1),
            duration_secs: 180,
            isrc: None,
            artists: None,
            explicit: false,
            extra: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn full_album_download_honors_track_quality_override() {
        let metadata = Arc::new(MockMetadataProvider::new("mock"));
        *metadata.fetch_tracks_result.lock().await = Ok((
            vec![provider_track("trk-1", 1), provider_track("trk-2", 2)],
            HashMap::new(),
        ));

        let download = Arc::new(MockDownloadSource::new("mock"));
        let temp = tempfile::NamedTempFile::new().unwrap();
        fs::write(temp.path(), b"not-real-audio").await.unwrap();
        *download.resolve_result.lock().await =
            Ok(PlaybackInfo::LocalFile(temp.path().to_path_buf()));

        let mut registry = ProviderRegistry::new();
        registry.register_metadata(metadata);
        registry.register_download(download.clone());

        let (state, _tmp) = test_app_state_with_registry(registry).await;
        let artist = seed_artist(&state.db, "Artist").await;
        let album = seed_album(&state.db, artist.id, "Album").await;
        let tracks = seed_tracks(&state.db, album.id, 2).await;
        db::update_track_quality_override(&state.db, tracks[0].id, Some(Quality::Lossless))
            .await
            .unwrap();
        seed_album_provider_link(&state.db, album.id, "mock", "album-ext").await;

        state.monitored_artists.write().await.push(artist);
        state.monitored_albums.write().await.push(album.clone());

        let mut job = seed_job(&state.db, album.id, crate::models::DownloadStatus::Queued).await;
        job.quality = Quality::Low;
        super::download_album_job(&state, job).await.unwrap();

        let requested = download.requested.lock().await.clone();
        assert_eq!(requested.len(), 2);
        assert!(
            requested
                .iter()
                .any(|(id, q)| id == "trk-1" && *q == Quality::Lossless)
        );
        assert!(
            requested
                .iter()
                .any(|(id, q)| id == "trk-2" && *q == Quality::Low)
        );
    }

    #[tokio::test]
    async fn partial_album_download_honors_track_quality_override() {
        let metadata = Arc::new(MockMetadataProvider::new("mock"));
        *metadata.fetch_tracks_result.lock().await = Ok((
            vec![provider_track("trk-1", 1), provider_track("trk-2", 2)],
            HashMap::new(),
        ));

        let download = Arc::new(MockDownloadSource::new("mock"));
        let temp = tempfile::NamedTempFile::new().unwrap();
        fs::write(temp.path(), b"not-real-audio").await.unwrap();
        *download.resolve_result.lock().await =
            Ok(PlaybackInfo::LocalFile(temp.path().to_path_buf()));

        let mut registry = ProviderRegistry::new();
        registry.register_metadata(metadata);
        registry.register_download(download.clone());

        let (state, _tmp) = test_app_state_with_registry(registry).await;
        let artist = seed_artist(&state.db, "Artist").await;
        let mut album = seed_album(&state.db, artist.id, "Album").await;
        album.monitored = false;
        album.wanted = false;
        album.partially_wanted = true;
        db::upsert_album(&state.db, &album).await.unwrap();

        let tracks = seed_tracks(&state.db, album.id, 2).await;
        db::update_track_flags(&state.db, tracks[0].id, true, false)
            .await
            .unwrap();
        db::update_track_quality_override(&state.db, tracks[0].id, Some(Quality::Lossless))
            .await
            .unwrap();
        seed_album_provider_link(&state.db, album.id, "mock", "album-ext").await;

        state.monitored_artists.write().await.push(artist);
        state.monitored_albums.write().await.push(album.clone());

        let job = seed_job(&state.db, album.id, crate::models::DownloadStatus::Queued).await;
        super::download_album_job(&state, job).await.unwrap();

        let requested = download.requested.lock().await.clone();
        assert_eq!(requested, vec![("trk-1".to_string(), Quality::Lossless)]);
    }

    #[tokio::test]
    async fn fetch_album_cover_art_falls_back_to_album_cover_url() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/cover.jpg",
            get(|| async { ([("content-type", "image/jpeg")], vec![1_u8, 2, 3, 4]) }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let provider = MockMetadataProvider::new("mock");
        let link = AlbumProviderLink {
            id: uuid::Uuid::now_v7(),
            album_id: uuid::Uuid::now_v7(),
            provider: "mock".to_string(),
            external_id: "album-1".to_string(),
            external_url: None,
            external_title: None,
            cover_ref: None,
        };
        let album = MonitoredAlbum {
            id: link.album_id,
            artist_id: uuid::Uuid::now_v7(),
            artist_ids: Vec::new(),
            artist_credits: Vec::new(),
            title: "Album".to_string(),
            album_type: None,
            release_date: None,
            cover_url: Some(format!("http://{addr}/cover.jpg")),
            explicit: false,
            quality_override: None,
            monitored: false,
            acquired: false,
            wanted: false,
            partially_wanted: true,
            added_at: chrono::Utc::now(),
        };

        let bytes =
            super::fetch_album_cover_art(&reqwest::Client::new(), &provider, &link, Some(&album))
                .await;

        server.abort();

        assert_eq!(bytes, Some(vec![1_u8, 2, 3, 4]));
    }
}
