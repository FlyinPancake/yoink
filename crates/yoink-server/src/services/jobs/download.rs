use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
};

use crate::{
    api::{Quality, WantedStatus},
    db::{
        album, album_provider_link, artist, job, job_kind::JobKind, job_status::JobStatus,
        provider::Provider, track, track_artist, track_provider_link,
    },
    error::{AppError, AppResult},
    providers::{DownloadSource, DownloadTrackContext},
    services::{
        self,
        downloads::{
            TrackMetadata,
            io::{DownloadPayload, MediaContainer, get_album_dir, sniff_media_container},
            lyrics::{fetch_track_lyrics, write_lrc_sidecar},
            metadata::build_full_artist_string,
            write_audio_metadata,
        },
        jobs::{Job, metadata::fetch_album_cover_art},
    },
    state::AppState,
    util::provider_priority,
};
use sea_orm::{ColumnTrait, EntityLoaderTrait, EntityTrait, IntoActiveModel, QueryFilter};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct DownloadAlbumJobPayload {
    pub album_id: uuid::Uuid,
    pub provider: Provider,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct DownloadTrackJobPayload {
    pub track_id: uuid::Uuid,
    pub provider: Provider,
}

fn download_jobs() -> [JobKind; 2] {
    [JobKind::DownloadAlbum, JobKind::DownloadTrack]
}

enum PlannedTrackDownload {
    Id {
        track: track::ModelEx,
        external_ids: Vec<String>,
        quality: Quality,
    },
    Metadata {
        track: track::ModelEx,
        metadata: DownloadTrackContext,
        quality: Quality,
    },
}

impl PlannedTrackDownload {
    fn track(&self) -> &track::ModelEx {
        match self {
            PlannedTrackDownload::Id { track, .. } => track,
            PlannedTrackDownload::Metadata { track, .. } => track,
        }
    }

    fn quality(&self) -> Quality {
        match self {
            PlannedTrackDownload::Id { quality, .. } => *quality,
            PlannedTrackDownload::Metadata { quality, .. } => *quality,
        }
    }
}

async fn process_album_download_job(state: AppState, job: &mut job::Model) -> AppResult<()> {
    tracing::debug!("Processing album download job: {}", job.id);

    let Job::DownloadAlbum { payload } = &job.data else {
        tracing::error!("Invalid job data for album download job: {:?}", job.data);
        return Err(AppError::download(
            "prepare",
            "Invalid job data for album download job",
        ));
    };

    let attempt = job.attempts + 1;
    let job = job.clone().into_active_model().into_ex();
    let job = job
        .set_status(JobStatus::Running)
        .set_attempts(attempt)
        .update(&state.db)
        .await?;

    let Some(album) = album::Entity::load()
        .filter_by_id(payload.album_id)
        .with(album_provider_link::Entity)
        .with(track::Entity)
        .with((track::Entity, track_provider_link::Entity))
        .with((track::Entity, track_artist::Entity))
        .with((track::Entity, artist::Entity))
        .one(&state.db)
        .await?
    else {
        tracing::error!(
            "Album not found for album download job: {}",
            payload.album_id
        );
        return Err(AppError::download(
            "prepare",
            "Album not found for album download job",
        ));
    };

    let Some(dl_provider) = state.registry.download_source(payload.provider) else {
        tracing::error!(
            "No download source found for provider {:?} in album download job",
            payload.provider
        );
        return Err(AppError::download(
            "prepare",
            "No download source found for provider in album download job",
        ));
    };

    let quality = album.requested_quality.unwrap_or(state.default_quality);

    let mut planned_tracks = VecDeque::new();

    if dl_provider.requires_linked_provider() {
        plan_tracks_by_id(payload, &album, quality, &mut planned_tracks)?
    } else {
        plan_tracks_by_metadata(&album, quality, &mut planned_tracks);
    }

    let album_artist = album
        .fetch_primary_artist(&state.db)
        .await?
        .map(|a| a.name)
        .unwrap_or("Unknown".to_string());

    let album_dir = get_album_dir(
        &state.music_root,
        &album_artist,
        &album.title,
        album.release_date,
    );

    let cover_art_jpeg = fetch_album_cover_art(&state, &album).await;

    let mut join_set = enqueue_tracks(state.clone(), dl_provider, planned_tracks).await?;

    let total_tracks = album.tracks.len() as f32;
    let mut completed_tracks = 0 as f32;

    let job = job.into_active_model();

    while let Some(result) = join_set.join_next().await {
        completed_tracks += 1.0;
        let job = job
            .clone()
            .set_progress(completed_tracks / total_tracks)
            .update(&state.db)
            .await?;
        state.notify_sse();

        let (track, temp_path, quality) = match result {
            Ok(Ok(pair)) => pair,
            Ok(Err(err)) => {
                tracing::error!(job_id = %job.id, error = %err, "Error downloading track in album download job");
                continue;
            }
            Err(err) => {
                tracing::error!(job_id = %job.id, error = %err, "Join error in album download job");
                continue;
            }
        };

        let path = move_downloaded_track(&album_dir, &track, temp_path, quality).await?;

        // TODO: having to refetch provider metadata here is not ideal
        // we should cache it during sync and only fill metadata from local db during download
        // it has worked like this so far, but it feels off
        let mut track_providers: Vec<_> = track.provider_links.iter().collect();
        track_providers.sort_by_key(|tp| provider_priority(tp.provider));
        let primary_provider = track_providers[0].provider;
        let primary_provider_id = &track_providers[0].provider_track_id;

        let md_provider = state
            .registry
            .metadata_provider(primary_provider)
            .expect("this should pass");

        let track_info_extra = md_provider
            .fetch_track_info_extra(primary_provider_id)
            .await;

        let track_artist = build_full_artist_string(
            &track.title,
            &track_info_extra.clone().unwrap_or_default(),
            None,
            &album_artist,
        );

        let lyrics = if state.download_lyrics {
            let duration_secs = match track.duration {
                Some(dur) if dur > 0 => Some(dur as u32),
                _ => None,
            };
            fetch_track_lyrics(
                &state,
                &track.title,
                &album_artist,
                &album.title,
                duration_secs,
            )
            .await
        } else {
            None
        };

        let release_date = album
            .release_date
            .map(|d| d.to_string())
            .unwrap_or("Unknown".to_string());

        let metadata = TrackMetadata {
            path: &path,
            title: &track.title,
            track_artist: &track_artist,
            album_artist: &album_artist,
            album: &album.title,
            track_number: track.track_number.map(|n| n as u32).unwrap_or(1),
            disc_number: track.disc_number.map(|n| n as u32),
            total_tracks: album.tracks.len() as u32,
            release_date: &release_date,
            // TODO remove these, as they are not used anymore
            track_extra: &HashMap::new(),
            album_extra: &HashMap::new(),
            track_info_extra: track_info_extra.as_ref(),
            lyrics_text: lyrics.as_ref().and_then(|b| b.embedded_text.as_deref()),
            cover_art_jpeg: cover_art_jpeg.as_deref(),
        };

        if let Err(e) = write_audio_metadata(&metadata) {
            tracing::warn!(
                track = %track.id,
                error = %e,
                "Failed to write audio metadata"
            );
        };

        if let Some(bundle) = lyrics
            && let Some(ref synced_lrc) = bundle.synced_lrc
            && let Err(err) = write_lrc_sidecar(&path, synced_lrc).await
        {
            tracing::warn!(
                track = %track.id,
                error = %err,
                "Failed to write LRC sidecar"
            )
        }

        let relative_path = path
            .strip_prefix(&state.music_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        let track = track
            .into_active_model()
            .set_status(WantedStatus::Acquired)
            .set_file_path(relative_path)
            .update(&state.db)
            .await?;
        state.notify_sse();

        tracing::debug!(
            track = %track.id,
            path = ?path,
            "Track downloaded and tagged"
        );
    }

    Ok(())
}

async fn move_downloaded_track(
    album_dir: &PathBuf,
    track: &track::ModelEx,
    temp_path: PathBuf,
    quality: Quality,
) -> Result<PathBuf, AppError> {
    let container = sniff_media_container(&temp_path).await?;

    // TODO maybe move this warning to the provider resolver, since if a provider is returning a non-flac hi-res stream, it's likely an error on its end
    if quality == Quality::HiRes
        && container != MediaContainer::Mp4
        && container != MediaContainer::Flac
    {
        tracing::warn!(
            track_id = %track.id,
            file = ?temp_path,
            "Hi Res output is not flac or M4a"
        );
    }

    let file_name = {
        let prefix = if let Some(disc_number) = track.disc_number {
            format!("{} - ", disc_number)
        } else {
            String::new()
        };
        let file_ext = match container.ext() {
            Some(ext) => ext,
            None => match quality {
                Quality::HiRes | Quality::Lossless => "flac",
                Quality::High | Quality::Low => "mp3",
            },
        };
        let track_number = track.track_number.unwrap_or(0);
        let title = services::downloads::io::sanitize_path_component(&track.title);
        format!("{prefix}{track_number:02} - {title}.{file_ext}")
    };
    let full_path = album_dir.join(file_name);
    tokio::fs::rename(&temp_path, &full_path)
        .await
        .map_err(|err| {
            AppError::filesystem(
                "move downloaded track to final location",
                format!("{} to {}", temp_path.display(), full_path.display()),
                err,
            )
        })?;
    Ok(full_path)
}

async fn enqueue_tracks(
    state: AppState,
    dl_provider: Arc<dyn DownloadSource>,
    planned_tracks: VecDeque<PlannedTrackDownload>,
) -> Result<JoinSet<AppResult<(track::ModelEx, PathBuf, Quality)>>, AppError> {
    let mut join_set = tokio::task::JoinSet::new();
    let semaphore = Arc::new(tokio::sync::Semaphore::new(
        state.download_max_parallel_tracks.max(1),
    ));
    let temp_dir = tempfile::tempdir()?;
    for track in planned_tracks {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("aquire DL permit shouldn't fail");
        let dl_provider = dl_provider.clone();
        let temp_dir = temp_dir.path().to_path_buf();
        let state = state.clone();

        join_set.spawn(async move {
            let temp_path = temp_dir.join(format!("{}.part", track.track().title));
            let quality = track.quality();
            let (track_model, pb_info) = match track {
                PlannedTrackDownload::Id {
                    external_ids,
                    quality,
                    track,
                } => (
                    track,
                    dl_provider.resolve_by_id(&external_ids, &quality).await?,
                ),
                PlannedTrackDownload::Metadata {
                    metadata,
                    quality,
                    track,
                } => (
                    track,
                    dl_provider.resolve_by_metadata(&metadata, &quality).await?,
                ),
            };

            match pb_info {
                crate::providers::PlaybackInfo::DirectUrl(url) => {
                    let payload = DownloadPayload::DirectUrl(url);
                    services::downloads::io::download_payload_to_file(
                        &state.http,
                        &payload,
                        &temp_path,
                    )
                    .await?;
                }
                crate::providers::PlaybackInfo::SegmentUrls(items) => {
                    let payload = DownloadPayload::DashSegmentUrls(items);
                    services::downloads::io::download_payload_to_file(
                        &state.http,
                        &payload,
                        &temp_path,
                    )
                    .await?;
                }
                crate::providers::PlaybackInfo::LocalFile(path_buf) => {
                    tokio::fs::copy(&path_buf, &temp_path)
                        .await
                        .map_err(|err| {
                            AppError::filesystem(
                                "copy local file for download",
                                format!("{} to {}", path_buf.display(), temp_path.display()),
                                err,
                            )
                        })?;
                }
            }

            drop(permit);
            AppResult::Ok((track_model, temp_path, quality))
        });
    }
    Ok(join_set)
}

fn plan_tracks_by_id(
    payload: &DownloadAlbumJobPayload,
    album: &album::ModelEx,
    quality: Quality,
    planned_tracks: &mut VecDeque<PlannedTrackDownload>,
) -> Result<(), AppError> {
    if album
        .provider_links
        .iter()
        .all(|apl| apl.provider != payload.provider)
    {
        tracing::error!(
            "Album {} does not have a linked provider {:?} required for download",
            album.id,
            payload.provider
        );
        return Err(AppError::download(
            "prepare",
            "Album does not have a linked provider required for download",
        ));
    }
    album.tracks.iter().for_each(|tr| {
        let external_ids: Vec<_> = tr
            .provider_links
            .iter()
            .filter_map(|pl| {
                if pl.provider == payload.provider {
                    Some(pl.provider_track_id.clone())
                } else {
                    None
                }
            })
            .collect();
        planned_tracks.push_back(PlannedTrackDownload::Id {
            track: tr.clone(),
            external_ids,
            quality,
        });
    });

    Ok(())
}

fn plan_tracks_by_metadata(
    album: &album::ModelEx,
    quality: Quality,
    planned_tracks: &mut VecDeque<PlannedTrackDownload>,
) {
    album.tracks.iter().for_each(|tr| {
        let primary_artist_id = tr
            .track_artists
            .iter()
            .min_by_key(|ta| ta.priority)
            .map(|ta| ta.artist_id);
        let artist_name = if let Some(primary_artist_id) = primary_artist_id {
            tr.artists
                .iter()
                .find(|ar| ar.id == primary_artist_id)
                .map(|ar| ar.name.clone())
                .unwrap_or_else(|| "Unknown Artist".to_string())
        } else {
            "Unknown Artist".to_string()
        };

        let metadata = DownloadTrackContext {
            artist_name,
            album_title: album.title.clone(),
            track_title: tr.title.clone(),
            track_number: tr.track_number.map(|n| n as u32),
            album_track_count: Some(album.tracks.len()),
            duration_secs: tr.duration.map(|d| d as u32),
        };
        planned_tracks.push_back(PlannedTrackDownload::Metadata {
            track: tr.clone(),
            metadata,
            quality,
        });
    });
}

pub(crate) async fn download_worker(state: AppState) -> AppResult<()> {
    tracing::info!("Download worker starting");
    loop {
        if state.shutdown.is_cancelled() {
            tracing::info!("Download worker shutting down");
            return Ok(());
        }
        // fetch job
        let Some(mut job) = job::Entity::find()
            .filter(job::Column::JobKind.is_in(download_jobs()))
            .one(&state.db)
            .await?
        else {
            tokio::select! {
                _ = state.download_notify.notified() => {}
                _ = state.shutdown.cancelled() => return Ok(()),
            }
            continue;
        };

        let job_handle = match &job.data {
            Job::DownloadAlbum { .. } => process_album_download_job(state.clone(), &mut job),
            _ => {
                tracing::error!(
                    "Unexpected job kind for download worker: {:?}",
                    job.job_kind
                );
                continue;
            }
        };

        match job_handle.await {
            Ok(()) => {
                tracing::info!("Job {} completed successfully", job.id);
                // TODO update job status to completed
            }
            Err(err) => {
                tracing::error!(error = %err, "Job {} failed with error", job.id);
                // TODO update job status to failed, increment attempts, set error message
            }
        }
    }
}
