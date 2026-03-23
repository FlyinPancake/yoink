mod io;
mod lyrics;
mod metadata;
mod worker;

pub(crate) use io::sanitize_path_component;
pub(crate) use metadata::{TrackMetadata, write_audio_metadata};
use uuid::Uuid;

use std::collections::HashSet;

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityLoaderTrait, IntoActiveModel,
    QueryFilter,
};

use crate::{
    db::{self, download_status::DownloadStatus, provider::Provider, wanted_status::WantedStatus},
    error::{AppError, AppResult},
    services::downloads::worker::download_album_job,
    state::AppState,
};

/// Enqueue a download job for an album.
pub(crate) async fn enqueue_album_download(state: &AppState, album_id: Uuid) -> AppResult<()> {
    tracing::warn!(album_id = %album_id, "enqueue_album_download is currently stubbed out");

    let Some(album) = db::album::Entity::load()
        .filter_by_id(album_id)
        .with(db::download_job::Entity)
        .with(db::track::Entity)
        .with(db::album_provider_link::Entity)
        .one(&state.db)
        .await?
    else {
        return Err(AppError::NotFound {
            resource: "album".to_string(),
            id: Some(album_id.to_string()),
        });
    };

    if album.wanted_status == WantedStatus::Unmonitored
        && album
            .tracks
            .iter()
            .all(|t| t.status == WantedStatus::Unmonitored)
    {
        return Err(AppError::DownloadPipeline {
            stage: "enqueue".into(),
            reason: "cannot enqueue download for an album marked as Unwanted".into(),
        });
    }

    if album.download_jobs.iter().any(|j| j.status.in_progress()) {
        return Err(AppError::Validation {
            field: None,
            reason: "a download job for this album is already in progress".into(),
        });
    }

    let quality = album.requested_quality.unwrap_or(state.default_quality);

    let source = {
        let download_sources = state.registry.download_sources();

        let download_source_ids: HashSet<_> = download_sources.iter().map(|s| s.id()).collect();

        let priority_source = album
            .provider_links
            .iter()
            .find(|l| download_source_ids.contains(&l.provider));

        if let Some(link) = priority_source {
            link.provider
        } else {
            download_sources
                .iter()
                .find(|s| !s.requires_linked_provider())
                .map(|s| s.id())
                .or_else(|| download_source_ids.iter().next().cloned())
                .unwrap_or(Provider::Tidal)
        }
    };

    let total_tracks = album.tracks.len() as i32;

    let job = db::download_job::ActiveModel {
        album_id: Set(album.id),
        source: Set(source),
        quality: Set(quality),
        total_tracks: Set(total_tracks),
        completed_tasks: Set(0),
        status: Set(DownloadStatus::Queued),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    tracing::info!(?job, "Enqueued download job for album");

    state.download_notify.notify_one();

    Ok(())
}

/// Background download worker loop.
pub(crate) async fn download_worker_loop(state: AppState) -> AppResult<()> {
    tracing::warn!("download worker started");
    loop {
        let Some(job) = db::download_job::Entity::load()
            .with(db::album::Entity)
            .with((db::album::Entity, db::album_provider_link::Entity))
            .filter(db::download_job::Column::Status.eq(DownloadStatus::Queued))
            .one(&state.db)
            .await?
        else {
            state.download_notify.notified().await;
            continue;
        };

        // Mark job as in-progress before starting work to prevent multiple workers from picking up the same job
        let job = job
            .into_active_model()
            .set_status(DownloadStatus::Resolving)
            .update(&state.db)
            .await?;

        if let Some(album) = job.album.as_ref()
            && album.wanted_status == WantedStatus::Wanted
        {
            let album_id = album.id;

            if let Err(e) = album
                .clone()
                .into_active_model()
                .set_wanted_status(WantedStatus::InProgress)
                .update(&state.db)
                .await
            {
                tracing::error!(album_id = %album_id, error = %e, "Failed to update album wanted status to InProgress at start of download job");
            }
        }

        tracing::info!(job_id = %job.id, album_id = %job.album_id, "Starting download job");

        // TODO partially wanted not yet supported correctly
        match download_album_job(&state, job.clone()).await {
            Ok(_) => {
                let job_id = job.id;

                let mut job = match job
                    .into_active_model()
                    .set_status(DownloadStatus::Completed)
                    .update(&state.db)
                    .await
                {
                    Ok(j) => j,
                    Err(e) => {
                        tracing::error!(job_id = ?job_id, error = %e, "Failed to update download job status to Completed");
                        continue;
                    }
                };
                tracing::info!(job_id = %job_id, "Download job completed successfully");

                let Some(album) = job.album.take() else {
                    tracing::error!(job_id = %job_id, "Download job has no associated album loaded");
                    continue;
                };

                if album.wanted_status == WantedStatus::Wanted {
                    let album_id = album.id;

                    if let Err(e) = album
                        .into_active_model()
                        .set_wanted_status(WantedStatus::Acquired)
                        .update(&state.db)
                        .await
                    {
                        tracing::error!(album_id = %album_id, error = %e, "Failed to update album wanted status to Downloaded after successful download");
                    } else {
                        tracing::info!(album_id = %album_id, "Album wanted status updated to Downloaded after successful download");
                    }
                }
            }
            Err(e) => {
                tracing::error!(job_id = %job.id, error = %e, "Download job failed");
                let job_id = job.id;
                let Ok(mut job) = job
                    .into_active_model()
                    .set_status(DownloadStatus::Failed)
                    .update(&state.db)
                    .await
                else {
                    tracing::error!(%job_id, error = %e, "Failed to update download job status to Failed after job failure");
                    continue;
                };

                if let Some(album) = job.album.take()
                    && album.wanted_status == WantedStatus::InProgress
                {
                    let album_id = album.id;

                    if let Err(e) = album
                        .into_active_model()
                        .set_wanted_status(WantedStatus::Wanted)
                        .update(&state.db)
                        .await
                    {
                        tracing::error!(album_id = %album_id, error = %e, "Failed to update album wanted status back to Wanted after download job failure");
                    } else {
                        tracing::info!(album_id = %album_id, "Album wanted status updated back to Wanted after download job failure");
                    }
                }
            }
        }
        state.notify_sse();
    }
}

/// Retag existing audio files with updated metadata.
///
/// TODO: rewrite to use SeaORM entities
pub(crate) async fn retag_existing_files(state: &AppState) -> AppResult<(usize, usize, usize)> {
    tracing::warn!("retag_existing_files is currently stubbed out");
    let _ = state;
    Ok((0, 0, 0))
}

/// Remove downloaded album files from disk.
///
/// TODO: rewrite to use SeaORM entities
pub(crate) async fn remove_downloaded_album_files(
    state: &AppState,
    album: &db::album::Model,
) -> AppResult<bool> {
    tracing::warn!(album_id = %album.id, "remove_downloaded_album_files is currently stubbed out");
    let _ = state;
    Ok(false)
}
