use std::{collections::VecDeque, path::Path, sync::Arc, time::Duration};

use crate::{
    db::{self, download_status::DownloadStatus, quality::Quality, wanted_status::WantedStatus},
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
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter};
use tokio::{sync::Semaphore, task::JoinSet};
use uuid::Uuid;

use super::lyrics::{fetch_track_lyrics, write_lrc_sidecar};

const EXDEV_ERROR_CODE: i32 = 18;

struct PlannedTrack {
    provider_track: crate::providers::ProviderTrack,
    db_track: db::track::Model,
}

struct DownloadContext {
    source: Arc<dyn crate::providers::DownloadSource>,
    metadata_provider: Arc<dyn crate::providers::MetadataProvider>,
    album: db::album::ModelEx,
    artist: db::artist::Model,
    external_album_id: String,
}

pub(crate) async fn download_album_job(
    state: &AppState,
    job: db::download_job::ModelEx,
) -> AppResult<()> {
    let context = load_download_context(state, &job).await?;
    let (provider_tracks, album_extra) = context
        .metadata_provider
        .fetch_tracks(&context.external_album_id)
        .await?;

    if provider_tracks.is_empty() {
        return Err(AppError::download(
            "fetch_metadata",
            "no tracks found for album from metadata provider",
        ));
    }

    let planned_tracks =
        build_album_download_plan(state, &provider_tracks, context.album.id).await?;
    run_download_plan(state, job, context, planned_tracks, album_extra).await
}

pub(crate) async fn download_track_job(
    state: &AppState,
    job: db::download_job::ModelEx,
) -> AppResult<()> {
    let track_id = job.track_id.ok_or_else(|| {
        AppError::validation(None::<String>, "track download job is missing track_id")
    })?;
    let context = load_download_context(state, &job).await?;
    let (provider_tracks, album_extra) = context
        .metadata_provider
        .fetch_tracks(&context.external_album_id)
        .await?;

    if provider_tracks.is_empty() {
        return Err(AppError::download(
            "fetch_metadata",
            "no tracks found for album from metadata provider",
        ));
    }

    let planned_track =
        build_track_download_plan(state, &provider_tracks, context.album.id, track_id).await?;
    let mut planned_tracks = VecDeque::with_capacity(1);
    planned_tracks.push_back(planned_track);

    run_download_plan(state, job, context, planned_tracks, album_extra).await
}

async fn load_download_context(
    state: &AppState,
    job: &db::download_job::ModelEx,
) -> AppResult<DownloadContext> {
    let source = state.registry.download_source(job.source).ok_or_else(|| {
        AppError::unavailable(
            "download source",
            format!("'{}' is not available", job.source),
        )
    })?;

    let Some(album) = job.album.as_ref().cloned() else {
        return Err(AppError::not_found("album", Some(job.album_id.to_string())));
    };

    let (metadata_link, metadata_provider) = album
        .provider_links
        .iter()
        .find_map(|link| {
            let metadata_provider = state.registry.metadata_provider(link.provider);
            match metadata_provider {
                Some(provider) if link.provider == job.source => Some((link, provider)),
                _ => None,
            }
        })
        .or_else(|| {
            album
                .provider_links
                .iter()
                .filter_map(|link| {
                    state
                        .registry
                        .metadata_provider(link.provider)
                        .map(|provider| (link, provider))
                })
                .max_by_key(|(link, _)| provider_priority(link.provider))
        })
        .ok_or_else(|| AppError::not_found("metadata_provider_link", Some(album.id.to_string())))?;

    let artist: db::artist::Model = album
        .fetch_primary_artist(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("artist", Some(album.id.to_string())))?;

    let external_album_id = metadata_link.provider_album_id.clone();

    Ok(DownloadContext {
        source,
        metadata_provider,
        album,
        artist,
        external_album_id,
    })
}

async fn build_album_download_plan(
    state: &AppState,
    provider_tracks: &[crate::providers::ProviderTrack],
    album_id: Uuid,
) -> AppResult<VecDeque<PlannedTrack>> {
    let mut planned_tracks = VecDeque::new();

    for provider_track in provider_tracks.iter().cloned() {
        let Some(existing_track) = match_local_track(state, &provider_track, album_id).await?
        else {
            continue;
        };

        planned_tracks.push_back(PlannedTrack {
            provider_track,
            db_track: existing_track,
        });
    }

    if planned_tracks.is_empty() {
        return Err(AppError::not_found(
            "matching provider tracks",
            Some(album_id.to_string()),
        ));
    }

    Ok(planned_tracks)
}

async fn build_track_download_plan(
    state: &AppState,
    provider_tracks: &[crate::providers::ProviderTrack],
    album_id: Uuid,
    track_id: Uuid,
) -> AppResult<PlannedTrack> {
    for provider_track in provider_tracks.iter().cloned() {
        let Some(existing_track) = match_local_track(state, &provider_track, album_id).await?
        else {
            continue;
        };

        if existing_track.id == track_id {
            return Ok(PlannedTrack {
                provider_track,
                db_track: existing_track,
            });
        }
    }

    Err(AppError::not_found("track", Some(track_id.to_string())))
}

async fn run_download_plan(
    state: &AppState,
    job: db::download_job::ModelEx,
    context: DownloadContext,
    planned_tracks: VecDeque<PlannedTrack>,
    album_extra: std::collections::HashMap<String, serde_json::Value>,
) -> AppResult<()> {
    let job_id = job.id;
    let album_id = context.album.id;
    let quality = job.quality;
    let total_tracks = planned_tracks.len() as i32;

    let job = job
        .into_active_model()
        .set_status(DownloadStatus::Downloading)
        .set_total_tracks(total_tracks)
        .set_completed_tasks(0)
        .update(&state.db)
        .await?;
    state.notify_sse();

    let release_suffix = context
        .album
        .release_date
        .map(|date| date.to_string())
        .unwrap_or("Unknown".to_string());

    let artist_dir = state
        .music_root
        .join(sanitize_path_component(&context.artist.name));
    let album_dir = artist_dir.join(format!("{} ({})", context.album.title, release_suffix));
    tokio::fs::create_dir_all(&album_dir).await.map_err(|err| {
        AppError::filesystem(
            "create_output_directory",
            album_dir.display().to_string(),
            err,
        )
    })?;

    let cover_art_jpeg = fetch_album_cover_art(
        state,
        &context.metadata_provider,
        &album_extra,
        &context.album,
    )
    .await;

    let mut join_set: JoinSet<AppResult<_>> = JoinSet::new();
    let semaphore = Arc::new(Semaphore::new(state.download_max_parallel_tracks.max(1)));
    let temp_dir = tempfile::tempdir()?;

    for track in planned_tracks {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let state = state.clone();
        let external_track_id = track.provider_track.external_id.clone();
        let quality = job.quality;
        let temp_dir = temp_dir.path().to_path_buf();
        let source = context.source.clone();

        join_set.spawn(async move {
            let playback_info = source
                .resolve_playback(&external_track_id, &quality, None)
                .await?;

            let temp_path = temp_dir.join(format!("{}.part", track.provider_track.title));

            let payload = match playback_info {
                PlaybackInfo::DirectUrl(url) => Some(DownloadPayload::DirectUrl(url)),
                PlaybackInfo::SegmentUrls(items) => Some(DownloadPayload::DashSegmentUrls(items)),
                PlaybackInfo::LocalFile(path_buf) => {
                    tokio::fs::copy(&path_buf, &temp_path)
                        .await
                        .map_err(|err| AppError::Filesystem {
                            operation: "copy downloaded track".to_string(),
                            path: path_buf.display().to_string(),
                            source: err,
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

    let mut completed_tracks = 0;
    let mut failed_tracks = 0;

    while let Some(result) = join_set.join_next().await {
        let (track, temp_path) = match result {
            Ok(Ok(pair)) => pair,
            Ok(Err(err)) => {
                failed_tracks += 1;
                tracing::error!(job_id = %job_id, error = %err, "Track download failed");
                continue;
            }
            Err(join_err) => {
                failed_tracks += 1;
                tracing::error!(job_id = %job_id, error = %join_err, "Track download task panicked");
                continue;
            }
        };

        completed_tracks += 1;
        job.clone()
            .into_active_model()
            .set_completed_tasks(completed_tracks)
            .update(&state.db)
            .await?;
        state.notify_sse();

        let container = sniff_media_container(&temp_path).await?;

        if quality == Quality::HiRes {
            let is_flac = has_flac_stream_marker(&temp_path)
                .await
                .map_err(|err| AppError::download("validate_track_format", err.to_string()))?;

            if !is_flac && container != MediaContainer::Mp4 {
                tracing::warn!(
                    track_id = %track.provider_track.external_id,
                    album_id = %album_id,
                    file = ?temp_path,
                    "HI_RES output is not FLAC"
                );
            }
        }

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

        move_downloaded_track(&temp_path, &final_path)
            .await
            .map_err(|err| {
                AppError::filesystem(
                    "move_downloaded_track",
                    final_path.display().to_string(),
                    err,
                )
            })?;

        let track_info_extra = context
            .metadata_provider
            .fetch_track_info_extra(&track.provider_track.external_id)
            .await;

        let track_artist = build_full_artist_string(
            &track.provider_track.title,
            &track.provider_track.extra,
            track_info_extra.as_ref(),
            &context.artist.name,
        );

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
                &context.album.title,
                duration,
            )
            .await
        } else {
            None
        };

        if let Err(err) = write_audio_metadata(&TrackMetadata {
            path: &final_path,
            title: &track.provider_track.title,
            track_artist: &track_artist,
            album_artist: &context.artist.name,
            album: &context.album.title,
            track_number: track.provider_track.track_number as u32,
            disc_number: track.provider_track.disc_number.map(|disc| disc as u32),
            total_tracks: total_tracks as u32,
            release_date: &release_suffix,
            track_extra: &track.provider_track.extra,
            album_extra: &album_extra,
            track_info_extra: track_info_extra.as_ref(),
            lyrics_text: lyrics
                .as_ref()
                .and_then(|bundle| bundle.embedded_text.as_deref()),
            cover_art_jpeg: cover_art_jpeg.as_deref(),
        }) {
            tracing::warn!(
                track = %track.provider_track.title,
                track_id = %track.provider_track.external_id,
                error = %err,
                "Failed to write audio metadata"
            );
        }

        if let Some(ref bundle) = lyrics
            && let Some(ref synced_lrc) = bundle.synced_lrc
            && let Err(err) = write_lrc_sidecar(&final_path, synced_lrc).await
        {
            tracing::warn!(
                track = %track.provider_track.title,
                error = %err,
                "Failed to write LRC sidecar"
            );
        }

        persist_downloaded_track(state, &track.db_track, &final_path).await?;
        state.notify_sse();

        tracing::debug!(
            track = %track.provider_track.title,
            track_number = track.provider_track.track_number,
            path = %final_path.display(),
            "Track downloaded and tagged"
        );
    }

    temp_dir.close()?;

    if failed_tracks > 0 {
        return Err(AppError::download(
            "download_tracks",
            format!("{failed_tracks} of {total_tracks} track(s) failed for download job {job_id}"),
        ));
    }

    Ok(())
}

async fn persist_downloaded_track(
    state: &AppState,
    track: &db::track::Model,
    final_path: &Path,
) -> AppResult<()> {
    let relative_path = final_path
        .strip_prefix(&state.music_root)
        .unwrap_or(final_path)
        .to_string_lossy()
        .to_string();

    let mut active = track.clone().into_active_model();
    active.status = sea_orm::ActiveValue::Set(WantedStatus::Acquired);
    active.file_path = sea_orm::ActiveValue::Set(Some(relative_path));
    active.update(&state.db).await?;

    Ok(())
}

async fn move_downloaded_track(temp_path: &Path, final_path: &Path) -> std::io::Result<()> {
    match tokio::fs::rename(temp_path, final_path).await {
        Ok(()) => Ok(()),
        Err(err) if is_cross_device_link_error(&err) => {
            tokio::fs::copy(temp_path, final_path).await?;
            tokio::fs::remove_file(temp_path).await?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn is_cross_device_link_error(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(EXDEV_ERROR_CODE)
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
    let cover_ref = album_extra
        .get("cover")
        .and_then(|value| value.as_str())
        .map(String::from);

    if let Some(ref image_ref) = cover_ref
        && let Some(bytes) = metadata_provider.fetch_cover_art_bytes(image_ref).await
    {
        tracing::debug!(
            image_ref,
            bytes = bytes.len(),
            "Fetched cover art via metadata provider"
        );
        return Some(bytes);
    }

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
                    tracing::debug!(url = %cover_url, bytes = bytes.len(), "Fetched cover art from cover_url");
                    return Some(bytes.to_vec());
                }
            }
            Ok(resp) => {
                tracing::warn!(url = %cover_url, status = %resp.status(), "cover_url returned non-success status");
            }
            Err(err) => {
                tracing::warn!(url = %cover_url, error = %err, "Failed to fetch cover art from cover_url");
            }
        }
    }

    tracing::debug!(album_id = %album.id, "No cover art available for album");
    None
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{is_cross_device_link_error, move_downloaded_track};

    #[test]
    fn detects_cross_device_link_errors() {
        assert!(is_cross_device_link_error(
            &std::io::Error::from_raw_os_error(18)
        ));
        assert!(!is_cross_device_link_error(&std::io::Error::other(
            "different error"
        )));
    }

    #[tokio::test]
    async fn move_downloaded_track_renames_within_same_filesystem() {
        let dir = tempdir().expect("create temp dir");
        let source = dir.path().join("source.flac");
        let target = dir.path().join("target.flac");

        tokio::fs::write(&source, b"audio")
            .await
            .expect("write source");

        move_downloaded_track(&source, &target)
            .await
            .expect("move track");

        assert!(!tokio::fs::try_exists(&source).await.expect("check source"));
        assert!(tokio::fs::try_exists(&target).await.expect("check target"));
        assert_eq!(
            tokio::fs::read(&target).await.expect("read target"),
            b"audio"
        );
    }
}
