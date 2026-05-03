use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    api::{Quality, WantedStatus},
    db::{
        self, album, album_provider_link, artist, job, job_kind::JobKind, job_status::JobStatus,
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
        jobs::{Job, enqueue_job, metadata::fetch_album_cover_art},
    },
    state::AppState,
    util::provider_priority,
};
use chrono::Utc;
use sea_orm::{
    ColumnTrait, EntityLoaderTrait, EntityTrait, ExprTrait, IntoActiveModel, QueryFilter,
    QueryOrder, QuerySelect, sea_query,
};
use serde::{Deserialize, Serialize};
use tokio::{io, task::JoinSet};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub(crate) struct DownloadAlbumJobPayload {
    pub album_id: uuid::Uuid,
    pub provider: Provider,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub(crate) struct DownloadTrackJobPayload {
    pub track_id: uuid::Uuid,
    pub provider: Provider,
}

fn download_jobs() -> [JobKind; 2] {
    [JobKind::DownloadAlbum, JobKind::DownloadTrack]
}

fn ready_jobs() -> [JobStatus; 2] {
    [JobStatus::Queued, JobStatus::Failed]
}

#[derive(Debug, Clone)]
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

pub(crate) async fn enqueue_download_album_job(
    state: &AppState,
    album_id: Uuid,
) -> AppResult<job::Model> {
    let providers: HashSet<_> = db::album_provider_link::Entity::find()
        .select_only()
        .column(db::album_provider_link::Column::Provider)
        .filter(db::album_provider_link::Column::AlbumId.eq(album_id))
        .distinct()
        .into_tuple::<Provider>()
        .all(&state.db)
        .await?
        .into_iter()
        .collect();

    let mut providers = state
        .registry
        .download_sources()
        .iter()
        .filter_map(|s| {
            let p = s.id();
            if providers.contains(&p) || !s.requires_linked_provider() {
                Some(p)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    providers.sort_by_key(|p| Reverse(provider_priority(*p)));

    let provider = providers
        .first()
        .copied()
        .ok_or(AppError::download("provider", "no providers available"))?;

    let payload = DownloadAlbumJobPayload { album_id, provider };
    let job = Job::DownloadAlbum { payload };
    enqueue_job(state, job).await
}

pub(crate) async fn enqueue_download_track_job(
    state: &AppState,
    track_id: Uuid,
) -> AppResult<job::Model> {
    let providers: HashSet<_> = db::track_provider_link::Entity::find()
        .select_only()
        .column(db::track_provider_link::Column::Provider)
        .distinct()
        .filter(db::track_provider_link::Column::TrackId.eq(track_id))
        .into_tuple::<Provider>()
        .all(&state.db)
        .await?
        .into_iter()
        .collect();

    let mut providers: Vec<_> = state
        .registry
        .download_sources()
        .iter()
        .filter_map(|s| {
            let p = s.id();
            if providers.contains(&p) || !s.requires_linked_provider() {
                Some(p)
            } else {
                None
            }
        })
        .collect();

    providers.sort_by_key(|p| Reverse(provider_priority(*p)));

    let provider = providers
        .first()
        .copied()
        .ok_or(AppError::download("provider", "no providers available"))?;

    let payload = DownloadTrackJobPayload { track_id, provider };
    let job = Job::DownloadTrack { payload };
    enqueue_job(state, job).await
}

pub(crate) async fn retry_album_download(state: &AppState, album_id: Uuid) -> AppResult<()> {
    let album_dl_job = job::Entity::load()
        .filter(
            job::COLUMN
                .deduplication_key
                .contains(format!("album:{}", album_id)),
        )
        .filter(job::COLUMN.job_kind.eq(JobKind::DownloadAlbum))
        .one(&state.db)
        .await?;

    if let Some(album_dl_job) = album_dl_job {
        if album_dl_job.status == JobStatus::Running {
            return Err(AppError::download("album", "download already in progress"));
        } else {
            album_dl_job.delete(&state.db).await?;
        }
    }

    // enqueue a new DL job for the album
    enqueue_download_album_job(state, album_id).await?;
    Ok(())
}

async fn process_album_download_job(state: AppState, job: job::ModelEx) -> AppResult<job::ModelEx> {
    tracing::debug!("Processing album download job: {}", job.id);

    let Job::DownloadAlbum { payload } = &job.data.clone() else {
        tracing::error!("Invalid job data for album download job: {:?}", job.data);
        return Err(AppError::download(
            "prepare",
            "Invalid job data for album download job",
        ));
    };

    let attempt = job.attempts + 1;

    let job = job
        .into_active_model()
        .set_status(JobStatus::Running)
        .set_attempts(attempt)
        .set_started_at(Utc::now())
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

    tokio::fs::create_dir_all(&album_dir).await?;

    let cover_art_jpeg = fetch_album_cover_art(&state, &album).await;

    let temp_dir = tempfile::tempdir()?;

    let mut join_set = enqueue_tracks(
        state.clone(),
        dl_provider,
        temp_dir.path().to_path_buf(),
        planned_tracks,
    )
    .await?;

    let total_tracks = album.tracks.len() as f32;
    let mut completed_tracks = 0.0_f32;

    // let job = job.into_active_model();
    let mut job = job;

    while let Some(result) = join_set.join_next().await {
        completed_tracks += 1.0;
        job = job
            .into_active_model()
            .set_progress(completed_tracks / total_tracks)
            .update(&state.db)
            .await?;
        state.notify_sse();

        let dl_result = match result {
            Ok(Ok(plan)) => Ok(plan),
            Ok(Err(err)) => {
                tracing::error!(job_id = %job.id, error = %err, "Error downloading track in album download job");
                Err(err)
            }
            Err(err) => {
                tracing::error!(job_id = %job.id, error = %err, "Join error in album download job");
                Err(AppError::download("download", err.to_string()))
            }
        };

        let (track, temp_path, quality) = match dl_result {
            Ok(plan) => plan,
            Err(err) => {
                return Err(err);
            }
        };

        let path = move_downloaded_track(&album_dir, &track, temp_path, quality).await?;

        enrich_track_metadata(
            &state,
            &album,
            &album_artist,
            &cover_art_jpeg,
            &track,
            &path,
        )
        .await?;

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

    drop(temp_dir);

    album
        .into_active_model()
        .set_wanted_status(WantedStatus::Acquired)
        .update(&state.db)
        .await?;
    state.notify_sse();

    Ok(job)
}

async fn process_track_download_job(state: AppState, job: job::ModelEx) -> AppResult<job::ModelEx> {
    tracing::debug!("Processing track download job: {}", job.id);

    let Job::DownloadTrack { payload } = job.data.clone() else {
        return Err(AppError::download(
            "prepare",
            "Invalid job data for track download job",
        ));
    };

    let attempts = job.attempts + 1;

    let job = job
        .into_active_model()
        .set_status(JobStatus::Running)
        .set_attempts(attempts)
        .set_started_at(Utc::now())
        .update(&state.db)
        .await?;

    let Some(track) = track::Entity::find_by_id(payload.track_id)
        .one(&state.db)
        .await?
    else {
        return Err(AppError::not_found(
            "track",
            Some(payload.track_id.to_string()),
        ));
    };

    let Some(album) = album::Entity::load()
        .filter_by_id(track.album_id)
        .with(album_provider_link::Entity)
        .with(track::Entity)
        .with((track::Entity, track_provider_link::Entity))
        .with((track::Entity, track_artist::Entity))
        .with((track::Entity, artist::Entity))
        .one(&state.db)
        .await?
    else {
        return Err(AppError::not_found(
            "album",
            Some(track.album_id.to_string()),
        ));
    };

    let target_track = album
        .tracks
        .iter()
        .find(|track| track.id == payload.track_id)
        .cloned()
        .ok_or_else(|| AppError::not_found("track", Some(payload.track_id.to_string())))?;

    let Some(dl_provider) = state.registry.download_source(payload.provider) else {
        tracing::error!(
            "No download source found for provider {:?} in track download job",
            payload.provider
        );
        return Err(AppError::download(
            "prepare",
            "No download source found for provider in track download job",
        ));
    };

    let quality = target_track
        .quality_override
        .or(album.requested_quality)
        .unwrap_or(state.default_quality);

    let mut planned_tracks = VecDeque::with_capacity(1);
    if dl_provider.requires_linked_provider() {
        planned_tracks.push_back(plan_track_by_id(payload.provider, &target_track, quality)?);
    } else {
        planned_tracks.push_back(plan_track_by_metadata(&album, &target_track, quality));
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

    tokio::fs::create_dir_all(&album_dir).await?;

    let cover_art_jpeg = fetch_album_cover_art(&state, &album).await;

    let temp_dir = tempfile::tempdir()?;

    let total_tracks = planned_tracks.len() as f32;
    let mut join_set = enqueue_tracks(
        state.clone(),
        dl_provider,
        temp_dir.path().to_path_buf(),
        planned_tracks,
    )
    .await?;

    let mut completed_tracks = 0.0;
    let mut job = job;

    while let Some(result) = join_set.join_next().await {
        completed_tracks += 1.0;
        job = job
            .into_active_model()
            .set_progress(completed_tracks / total_tracks)
            .update(&state.db)
            .await?;
        state.notify_sse();

        let (track, temp_path, quality) = match result {
            Ok(Ok(pair)) => pair,
            Ok(Err(err)) => {
                tracing::error!(job_id = %job.id, error = %err, "Error downloading track in track download job");
                return Err(err);
            }
            Err(err) => {
                tracing::error!(job_id = %job.id, error = %err, "Join error in track download job");
                return Err(AppError::download("download_track", err.to_string()));
            }
        };

        let path = move_downloaded_track(&album_dir, &track, temp_path, quality).await?;

        enrich_track_metadata(
            &state,
            &album,
            &album_artist,
            &cover_art_jpeg,
            &track,
            &path,
        )
        .await?;

        let relative_path = path
            .strip_prefix(&state.music_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        track
            .into_active_model()
            .set_status(WantedStatus::Acquired)
            .set_file_path(relative_path)
            .update(&state.db)
            .await?;
        state.notify_sse();

        tracing::debug!(
            track = %payload.track_id,
            path = ?path,
            "Track downloaded and tagged"
        );
    }

    drop(temp_dir);

    services::downloads::sync_album_wanted_status_from_tracks(&state, album.id).await?;
    state.notify_sse();

    Ok(job)
}

async fn enrich_track_metadata(
    state: &AppState,
    album: &album::ModelEx,
    album_artist: &String,
    cover_art_jpeg: &Option<Vec<u8>>,
    track: &track::ModelEx,
    path: &PathBuf,
) -> AppResult<()> {
    // TODO: having to refetch provider metadata here is not ideal
    // we should cache it during sync and only fill metadata from local db during download
    // it has worked like this so far, but it feels off
    let mut track_providers: Vec<_> = track.provider_links.iter().collect();
    track_providers.sort_by_key(|tp| Reverse(provider_priority(tp.provider)));

    let Some(first_provider) = track_providers.first() else {
        return Err(AppError::metadata(
            "enrich metadata",
            "no provider links found",
        ));
    };

    let primary_provider = first_provider.provider;
    let primary_provider_id = first_provider.provider_track_id.as_ref();

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
        album_artist,
    );

    let lyrics = if state.download_lyrics {
        let duration_secs = match track.duration {
            Some(dur) if dur > 0 => Some(dur as u32),
            _ => None,
        };
        fetch_track_lyrics(
            state,
            &track.title,
            album_artist,
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
        path,
        title: &track.title,
        track_artist: &track_artist,
        album_artist,
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
        && let Err(err) = write_lrc_sidecar(path, synced_lrc).await
    {
        tracing::warn!(
            track = %track.id,
            error = %err,
            "Failed to write LRC sidecar"
        );
        return Err(AppError::metadata("write LRC sidecar", err.to_string()));
    }

    Ok(())
}

async fn move_downloaded_track(
    album_dir: &Path,
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
    match tokio::fs::rename(&temp_path, &full_path).await {
        Ok(_) => {}
        Err(err) if matches!(err.kind(), io::ErrorKind::CrossesDevices) => {
            tokio::fs::copy(&temp_path, &full_path).await?;
        }
        Err(err) => {
            return Err(AppError::filesystem(
                "move downloaded track to final location",
                format!("{} to {}", temp_path.display(), full_path.display()),
                err,
            ));
        }
    };
    Ok(full_path)
}

async fn enqueue_tracks(
    state: AppState,
    dl_provider: Arc<dyn DownloadSource>,
    temp_dir_path: PathBuf,
    planned_tracks: VecDeque<PlannedTrackDownload>,
) -> Result<JoinSet<AppResult<(track::ModelEx, PathBuf, Quality)>>, AppError> {
    let mut join_set = tokio::task::JoinSet::new();
    let semaphore = Arc::new(tokio::sync::Semaphore::new(
        state.download_max_parallel_tracks.max(1),
    ));
    for track in planned_tracks {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("aquire DL permit shouldn't fail");
        let dl_provider = dl_provider.clone();
        let temp_dir_path = temp_dir_path.clone();

        let state = state.clone();

        join_set.spawn(async move {
            let temp_path = temp_dir_path.join(format!("{}.part", track.track().title));
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

    for track in &album.tracks {
        planned_tracks.push_back(plan_track_by_id(payload.provider, track, quality)?);
    }

    Ok(())
}

fn plan_track_by_id(
    provider: Provider,
    track: &track::ModelEx,
    quality: Quality,
) -> Result<PlannedTrackDownload, AppError> {
    let external_ids: Vec<_> = track
        .provider_links
        .iter()
        .filter_map(|link| {
            if link.provider == provider {
                Some(link.provider_track_id.clone())
            } else {
                None
            }
        })
        .collect();

    if external_ids.is_empty() {
        tracing::error!(
            track = %track.id,
            provider = %provider,
            "Track does not have a linked provider required for download"
        );
        return Err(AppError::download(
            "prepare",
            "Track does not have a linked provider required for download",
        ));
    }

    Ok(PlannedTrackDownload::Id {
        track: track.clone(),
        external_ids,
        quality,
    })
}

fn plan_tracks_by_metadata(
    album: &album::ModelEx,
    quality: Quality,
    planned_tracks: &mut VecDeque<PlannedTrackDownload>,
) {
    for track in &album.tracks {
        planned_tracks.push_back(plan_track_by_metadata(album, track, quality));
    }
}

fn plan_track_by_metadata(
    album: &album::ModelEx,
    track: &track::ModelEx,
    quality: Quality,
) -> PlannedTrackDownload {
    let primary_artist_id = track
        .track_artists
        .iter()
        .min_by_key(|ta| ta.priority)
        .map(|ta| ta.artist_id);
    let artist_name = if let Some(primary_artist_id) = primary_artist_id {
        track
            .artists
            .iter()
            .find(|artist| artist.id == primary_artist_id)
            .map(|artist| artist.name.clone())
            .unwrap_or_else(|| "Unknown Artist".to_string())
    } else {
        "Unknown Artist".to_string()
    };

    let metadata = DownloadTrackContext {
        artist_name,
        album_title: album.title.clone(),
        track_title: track.title.clone(),
        track_number: track.track_number.map(|n| n as u32),
        album_track_count: Some(album.tracks.len()),
        duration_secs: track.duration.map(|d| d as u32),
    };

    PlannedTrackDownload::Metadata {
        track: track.clone(),
        metadata,
        quality,
    }
}

pub(crate) async fn download_worker(state: AppState) -> AppResult<()> {
    tracing::info!("Download worker starting");
    loop {
        if state.shutdown.is_cancelled() {
            tracing::info!("Download worker shutting down");
            return Ok(());
        }
        // fetch job
        // TODO implement back-off for jobs
        let Some(job) = job::Entity::find()
            .filter(job::Column::JobKind.is_in(download_jobs()))
            .filter(job::Column::Status.is_in(ready_jobs()))
            .filter(
                sea_query::Expr::col(job::Column::Attempts)
                    .lt(sea_query::Expr::col(job::Column::MaxAttempts)),
            )
            .order_by_asc(job::Column::CreatedAt)
            .one(&state.db)
            .await?
        else {
            tokio::select! {
                _ = state.download_notify.notified() => {}
                _ = state.shutdown.cancelled() => return Ok(()),
            }
            continue;
        };

        let job_result = match &job.data {
            Job::DownloadAlbum { .. } => {
                process_album_download_job(state.clone(), job.clone().into_ex()).await
            }
            Job::DownloadTrack { .. } => {
                process_track_download_job(state.clone(), job.clone().into_ex()).await
            }
        };

        let job = match job_result {
            Ok(job) => {
                tracing::info!("Job {} completed successfully", job.id);
                job.into_active_model().set_status(JobStatus::Succeeded)
            }
            Err(err) => {
                tracing::error!(error = %err, "Job {} failed with error", job.id);
                // TODO increment attempts instead of at the start of the job
                job.into_active_model()
                    .into_ex()
                    .set_status(JobStatus::Failed)
                    .set_error_message(format!("{err}"))
            }
        };

        job.set_finished_at(Utc::now()).update(&state.db).await?;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sea_orm::{ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set};

    use super::{enqueue_download_album_job, enqueue_download_track_job};
    use crate::{
        db::{self, provider::Provider, wanted_status::WantedStatus},
        providers::mock::TestDownloadSource,
        services::jobs::Job,
        test_support,
    };

    fn registry_with_tidal_and_soulseek() -> crate::providers::registry::ProviderRegistry {
        let mut registry = crate::providers::registry::ProviderRegistry::new();
        registry.register_download(Arc::new(TestDownloadSource::new(Provider::Soulseek, false)));
        registry.register_download(Arc::new(TestDownloadSource::new(Provider::Tidal, true)));
        registry
    }

    #[tokio::test]
    async fn enqueue_download_track_job_prefers_higher_priority_linked_provider() {
        let state =
            test_support::test_state_with_registry(registry_with_tidal_and_soulseek()).await;
        let album = test_support::seed_album(&state, "Tidal Album", WantedStatus::Wanted).await;
        let track =
            test_support::seed_track(&state, album.id, "Tidal Track", 1, WantedStatus::Wanted)
                .await;

        db::track_provider_link::ActiveModel {
            track_id: Set(track.id),
            provider: Set(Provider::Tidal),
            provider_track_id: Set("tidal-track-id".to_string()),
            ..db::track_provider_link::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert track provider link");

        let job = enqueue_download_track_job(&state, track.id)
            .await
            .expect("enqueue track download");

        let Job::DownloadTrack { payload } = job.data else {
            panic!("expected track download job");
        };
        assert_eq!(payload.provider, Provider::Tidal);
    }

    #[tokio::test]
    async fn enqueue_download_album_job_prefers_higher_priority_linked_provider() {
        let state =
            test_support::test_state_with_registry(registry_with_tidal_and_soulseek()).await;
        let album = test_support::seed_album(&state, "Tidal Album", WantedStatus::Wanted).await;

        db::album_provider_link::ActiveModel {
            album_id: Set(album.id),
            provider: Set(Provider::Tidal),
            provider_album_id: Set("tidal-album-id".to_string()),
            external_url: Set(None),
            external_name: Set(None),
            ..db::album_provider_link::ActiveModel::new()
        }
        .insert(&state.db)
        .await
        .expect("insert album provider link");

        let job = enqueue_download_album_job(&state, album.id)
            .await
            .expect("enqueue album download");

        let Job::DownloadAlbum { payload } = job.data else {
            panic!("expected album download job");
        };
        assert_eq!(payload.provider, Provider::Tidal);
    }
}
