use std::{collections::VecDeque, sync::Arc, time::Duration};

use crate::{
    db::{self, download_status::DownloadStatus, quality::Quality},
    error::{AppError, AppResult},
    providers::PlaybackInfo,
    services::{
        self,
        downloads::{
            TrackMetadata,
            io::{DownloadPayload, MediaContainer, has_flac_stream_marker, sniff_media_container},
            metadata::{build_full_artist_string, extract_disc_number},
            sanitize_path_component, write_audio_metadata,
        },
    },
    state::AppState,
    util::provider_priority,
};
use sea_orm::{ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter};
use tokio::{sync::Semaphore, task::JoinSet};
use uuid::Uuid;

use super::lyrics::{fetch_track_lyrics, write_lrc_sidecar};

struct PlannedTrack {
    provider_track: crate::providers::ProviderTrack,
    #[allow(dead_code)]
    db_track: db::track::Model,
}

/// Download all tracks for an album job.
///
/// FIXME: support for independent track downloads
pub(crate) async fn download_album_job(
    state: &AppState,
    job: db::download_job::ModelEx,
) -> AppResult<()> {
    let job_id = job.id;

    let quality = job.quality;

    let source = state.registry.download_source(job.source).ok_or_else(|| {
        AppError::unavailable(
            "download source",
            format!("'{}' is not available", job.source),
        )
    })?;

    let Some(album) = job.album.clone().take() else {
        return Err(AppError::not_found("album", Some(job.album_id.to_string())));
    };
    let album_id = album.id;

    let (metadata_link, metadata_provider) = album
        .provider_links
        .iter()
        .find_map(|l| {
            let metadata_provider = state.registry.metadata_provider(l.provider);
            match metadata_provider {
                Some(prov) => {
                    if l.provider == job.source {
                        Some((l, prov))
                    } else {
                        None
                    }
                }
                None => None,
            }
        })
        .or_else(|| {
            album
                .provider_links
                .iter()
                .filter_map(|l| {
                    let prov = state.registry.metadata_provider(l.provider);
                    prov.map(|p| (l, p))
                })
                .max_by_key(|(l, _)| provider_priority(l.provider))
        })
        .ok_or_else(|| AppError::not_found("metadata_provider_Link", Some(album.id)))?;

    let external_album_id = metadata_link.provider_album_id.clone();

    tracing::info!(?job_id, ?album_id, ?external_album_id, source = ?job.source, "starting download job");

    let (provider_tracks, album_extra) = metadata_provider.fetch_tracks(&external_album_id).await?;

    if provider_tracks.is_empty() {
        return Err(AppError::DownloadPipeline {
            stage: "fetch_metadata".into(),
            reason: "no tracks found for album from metadata provider".into(),
        });
    }

    let mut planned_tracks = VecDeque::new();

    for track in provider_tracks {
        let Some(existing_track) = match_local_track(state, &track, album_id).await? else {
            continue;
        };
        planned_tracks.push_back(PlannedTrack {
            provider_track: track,
            db_track: existing_track,
        });
    }

    if planned_tracks.is_empty() {
        return Err(AppError::not_found(
            "matching provider tracks",
            Some(album_id.to_string()),
        ));
    }

    let total_tracks = planned_tracks.len() as i32;

    let job = job
        .into_active_model()
        .set_status(DownloadStatus::Downloading)
        .set_total_tracks(total_tracks)
        .set_completed_tasks(0)
        .update(&state.db)
        .await?;

    let artist = album
        .fetch_primary_artist(&state.db)
        .await?
        .expect("album without artist");

    let release_suffix = album
        .release_date
        .map(|d| d.to_string())
        .unwrap_or("Unknown".to_string());

    let artist_dir = state.music_root.join(sanitize_path_component(&artist.name));
    let album_dir = artist_dir.join(format!("{} ({})", album.title, release_suffix));
    tokio::fs::create_dir_all(&album_dir).await.map_err(|err| {
        AppError::filesystem(
            "create_output_directory",
            album_dir.display().to_string(),
            err,
        )
    })?;

    // ── Fetch album cover art once (shared across all tracks) ─────────
    let cover_art_jpeg =
        fetch_album_cover_art(state, &metadata_provider, &album_extra, &album).await;

    // ── Download tracks in parallel ─────────────────────────────────────
    let mut join_set: JoinSet<AppResult<_>> = JoinSet::new();

    let semaphore = Arc::new(Semaphore::new(state.download_max_parallel_tracks.max(1)));

    let temp_dir = tempfile::tempdir()?;

    for track in planned_tracks {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let state = state.clone();
        let id = track.provider_track.external_id.clone();
        let quality = job.quality;
        let temp_dir = temp_dir.path().to_path_buf();
        let source = source.clone();

        join_set.spawn(async move {
            let pb_info = source.resolve_playback(&id, &quality, None).await?;

            let temp_path = temp_dir.join(format!("{}.part", track.provider_track.title));

            let payload = match pb_info {
                PlaybackInfo::DirectUrl(url) => Some(DownloadPayload::DirectUrl(url)),
                PlaybackInfo::SegmentUrls(items) => Some(DownloadPayload::DashSegmentUrls(items)),
                PlaybackInfo::LocalFile(path_buf) => {
                    tokio::fs::copy(&path_buf, &temp_path).await.map_err(|e| {
                        AppError::Filesystem {
                            operation: "copy downloaded track".to_string(),
                            path: path_buf.display().to_string(),
                            source: e,
                        }
                    })?;
                    None
                }
            };

            if let Some(payload) = payload {
                services::downloads::io::download_payload_to_file(
                    &state.http,
                    &payload,
                    &temp_path,
                )
                .await?;
            }

            drop(permit);
            Ok((track, temp_path))
        });
    }

    // ── Process completed downloads sequentially ────────────────────────
    let mut completed_tracks = 0;
    let mut failed_tracks = 0;

    while let Some(res) = join_set.join_next().await {
        let (track, temp_path) = match res {
            Ok(Ok(pair)) => pair,
            Ok(Err(err)) => {
                failed_tracks += 1;
                tracing::error!(
                    job_id = %job_id,
                    error = %err,
                    "track download failed"
                );
                continue;
            }
            Err(join_err) => {
                failed_tracks += 1;
                tracing::error!(
                    job_id = %job_id,
                    error = %join_err,
                    "track download task panicked"
                );
                continue;
            }
        };

        completed_tracks += 1;
        job.clone()
            .into_active_model()
            .set_completed_tasks(completed_tracks)
            .update(&state.db)
            .await?;

        let container = sniff_media_container(&temp_path).await?;

        if quality == Quality::HiRes {
            let is_flac = has_flac_stream_marker(&temp_path)
                .await
                .map_err(|err| AppError::download("validate_track_format", err.to_string()))?;

            if !is_flac && container != MediaContainer::Mp4 {
                tracing::warn!(
                    track_id = %track.provider_track.external_id,
                    album_id = %job.album_id,
                    file = ?temp_path,
                    "HI_RES output is not FLAC"
                );
            }
        }

        // ── Determine final file name and move ──────────────────────────
        let file_name = match track.provider_track.disc_number {
            Some(disc) => format!("{disc}-"),
            None => String::new(),
        } + &format!(
            "{:02} - {}",
            track.provider_track.track_number,
            sanitize_path_component(&track.provider_track.title)
        );

        let file_name = match container.ext() {
            Some(ext) => file_name + "." + ext,
            None => match quality {
                Quality::HiRes | Quality::Lossless => file_name + ".flac",
                Quality::High | Quality::Low => file_name + ".mp3",
            },
        };
        let final_path = album_dir.join(file_name);

        tokio::fs::rename(&temp_path, &final_path).await?;

        // ── Fetch per-track extra metadata ──────────────────────────────
        let track_info_extra = metadata_provider
            .fetch_track_info_extra(&track.provider_track.external_id)
            .await;

        let track_artist = build_full_artist_string(
            &track.provider_track.title,
            &track.provider_track.extra,
            track_info_extra.as_ref(),
            &artist.name,
        );

        // ── Fetch lyrics (conditionally) ────────────────────────────────
        let lyrics = if state.download_lyrics {
            let duration = if track.provider_track.duration_secs > 0 {
                Some(track.provider_track.duration_secs as u32)
            } else {
                None
            };
            fetch_track_lyrics(
                state,
                &track.provider_track.title,
                &track_artist,
                &album.title,
                duration,
            )
            .await
        } else {
            None
        };

        // ── Write audio metadata tags ───────────────────────────────────
        if let Err(err) = write_audio_metadata(&TrackMetadata {
            path: &final_path,
            title: &track.provider_track.title,
            track_artist: &track_artist,
            album_artist: &artist.name,
            album: &album.title,
            track_number: track.provider_track.track_number as u32,
            disc_number: track.provider_track.disc_number.map(|d| d as u32),
            total_tracks: total_tracks as u32,
            release_date: &release_suffix,
            track_extra: &track.provider_track.extra,
            album_extra: &album_extra,
            track_info_extra: track_info_extra.as_ref(),
            lyrics_text: lyrics.as_ref().and_then(|l| l.embedded_text.as_deref()),
            cover_art_jpeg: cover_art_jpeg.as_deref(),
        }) {
            tracing::warn!(
                track = %track.provider_track.title,
                track_id = %track.provider_track.external_id,
                error = %err,
                "failed to write audio metadata"
            );
        }

        // ── Write synced lyrics sidecar (.lrc) ──────────────────────────
        if let Some(ref bundle) = lyrics
            && let Some(ref synced_lrc) = bundle.synced_lrc
            && let Err(err) = write_lrc_sidecar(&final_path, synced_lrc).await
        {
            tracing::warn!(
                track = %track.provider_track.title,
                error = %err,
                "failed to write LRC sidecar"
            );
        }

        tracing::debug!(
            track = %track.provider_track.title,
            track_number = track.provider_track.track_number,
            path = %final_path.display(),
            "track downloaded and tagged"
        );
    }

    temp_dir.close()?;

    if failed_tracks > 0 {
        tracing::warn!(
            job_id = %job_id,
            completed = completed_tracks,
            failed = failed_tracks,
            "download job completed with failures"
        );
    }

    Ok(())
}

/// Attempt to find a local track matching the provided provider track, either via existing provider link or by matching ISRC.
async fn match_local_track(
    state: &AppState,
    track: &crate::providers::ProviderTrack,
    album_id: Uuid,
) -> AppResult<Option<db::track::Model>> {
    if let Some((_, Some(track))) = db::track_provider_link::Entity::find()
        .filter(db::track_provider_link::Column::ProviderTrackId.eq(track.external_id.clone()))
        .find_also_related(db::track::Entity)
        .one(&state.db)
        .await?
    {
        return Ok(Some(track));
    }

    if let Some(isrc) = track.isrc.as_deref()
        && let Some(track) = db::track::Entity::find()
            .filter(db::track::Column::Isrc.eq(isrc))
            .one(&state.db)
            .await?
    {
        return Ok(Some(track));
    }

    let disc_number = extract_disc_number(&track.extra, None).unwrap_or(1);
    let track = db::track::Entity::find()
        .filter(db::track::Column::AlbumId.eq(album_id))
        .filter(db::track::Column::DiscNumber.eq(disc_number))
        .filter(db::track::Column::TrackNumber.eq(track.track_number))
        .one(&state.db)
        .await?;

    Ok(track)
}

/// Fetch album cover art JPEG bytes for embedding into track tags.
///
/// Tries the metadata provider's `fetch_cover_art_bytes` first (using a cover
/// reference from `album_extra`), then falls back to downloading the album's
/// stored `cover_url` directly.
async fn fetch_album_cover_art(
    state: &AppState,
    metadata_provider: &Arc<dyn crate::providers::MetadataProvider>,
    album_extra: &std::collections::HashMap<String, serde_json::Value>,
    album: &db::album::ModelEx,
) -> Option<Vec<u8>> {
    // Try to extract a cover image reference from album_extra (provider-specific key).
    let cover_ref = album_extra
        .get("cover")
        .and_then(|v| v.as_str())
        .map(String::from);

    if let Some(ref image_ref) = cover_ref
        && let Some(bytes) = metadata_provider.fetch_cover_art_bytes(image_ref).await
    {
        tracing::debug!(
            image_ref,
            bytes = bytes.len(),
            "fetched cover art via metadata provider"
        );
        return Some(bytes);
    }

    // Fallback: download the album's cover_url directly.
    if let Some(ref cover_url) = album.cover_url {
        match state
            .http
            .get(cover_url.to_string())
            .timeout(Duration::from_secs(20))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(bytes) = resp.bytes().await {
                    tracing::debug!(
                        url = %cover_url,
                        bytes = bytes.len(),
                        "fetched cover art from cover_url"
                    );
                    return Some(bytes.to_vec());
                }
            }
            Ok(resp) => {
                tracing::warn!(
                    url = %cover_url,
                    status = %resp.status(),
                    "cover_url returned non-success status"
                );
            }
            Err(err) => {
                tracing::warn!(
                    url = %cover_url,
                    error = %err,
                    "failed to fetch cover art from cover_url"
                );
            }
        }
    }

    tracing::debug!(album_id = %album.id, "no cover art available for album");
    None
}
