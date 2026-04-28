pub mod download;
pub mod metadata;
use sea_orm::{
    ActiveEnum, ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    FromJsonQueryResult, QueryFilter,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    db::{job, job_kind::JobKind, job_status::JobStatus, provider::Provider},
    error::AppResult,
    state::AppState,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromJsonQueryResult, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Job {
    DownloadAlbum {
        #[serde(flatten)]
        payload: download::DownloadAlbumJobPayload,
    },
    DownloadTrack {
        #[serde(flatten)]
        payload: download::DownloadTrackJobPayload,
    },
}

impl Job {
    pub fn kind(&self) -> crate::db::job_kind::JobKind {
        match self {
            Job::DownloadAlbum { .. } => crate::db::job_kind::JobKind::DownloadAlbum,
            Job::DownloadTrack { .. } => crate::db::job_kind::JobKind::DownloadTrack,
        }
    }

    pub fn dedupe_key(&self) -> String {
        let prefix = self.kind().into_value();
        let suffix = match self {
            Job::DownloadAlbum { payload } => format!("album:{}", payload.album_id),
            Job::DownloadTrack { payload } => format!("track:{}", payload.track_id),
        };
        format!("{prefix}:{suffix}")
    }
}

const MAX_ATTEMPTS: i32 = 3;

pub(crate) async fn enqueue_job(state: &AppState, job: Job) -> AppResult<job::Model> {
    let deduplication_key = job.dedupe_key();
    let job = job::ActiveModel {
        data: Set(job),
        deduplication_key: Set(deduplication_key),
        max_attempts: Set(MAX_ATTEMPTS),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    state.download_notify.notify_one();

    Ok(job)
}

pub(crate) async fn list_jobs(db: &DatabaseConnection) -> AppResult<Vec<job::Model>> {
    let jobs = job::Entity::find().all(db).await?;
    Ok(jobs)
}

pub(crate) async fn list_jobs_for_album(
    db: &DatabaseConnection,
    album_id: Uuid,
) -> AppResult<Vec<job::Model>> {
    let jobs = job::Entity::find()
        .filter(job::Column::DeduplicationKey.contains(format!("album:{}", album_id)))
        .all(db)
        .await?;

    Ok(jobs)
}

pub(crate) async fn clear_completed_jobs(db: &DatabaseConnection) -> AppResult<()> {
    job::Entity::delete_many()
        .filter(job::Column::Status.eq(JobStatus::Succeeded))
        .exec(db)
        .await?;
    Ok(())
}

pub(crate) async fn clear_completed_album_jobs(
    db: &DatabaseConnection,
    album_id: Uuid,
) -> AppResult<()> {
    job::Entity::delete_many()
        .filter(job::Column::Status.eq(JobStatus::Succeeded))
        .filter(job::Column::DeduplicationKey.contains(format!("album:{}", album_id)))
        .exec(db)
        .await?;
    Ok(())
}

// TODO implement this function to cancel any pending jobs related to the album and its tracks when the album is unmonitored
// fail on any job that is currently running and cannot be cancelled
pub(crate) async fn prepare_album_for_unmonitor(state: &AppState, album_id: Uuid) -> AppResult<()> {
    tracing::warn!("prepare_album_for_unmonitor is not implemented yet");
    Ok(())
}

pub(crate) async fn enqueue_download_album_job(
    state: &AppState,
    album_id: Uuid,
) -> AppResult<job::Model> {
    // FIXME: hardcoded provider, should be fetched from the album record
    let payload = download::DownloadAlbumJobPayload {
        album_id,
        provider: crate::db::provider::Provider::Tidal,
    };
    let job = Job::DownloadAlbum { payload };
    enqueue_job(state, job).await
}

pub(crate) async fn enqueue_download_track_job(
    state: &AppState,
    track_id: Uuid,
    provider: Provider,
) -> AppResult<job::Model> {
    let payload = download::DownloadTrackJobPayload { track_id, provider };
    let job = Job::DownloadTrack { payload };
    enqueue_job(state, job).await
}

pub(crate) async fn retry_album_download(state: &AppState, album_id: Uuid) -> AppResult<()> {
    // delete DL jobs for the album
    job::Entity::delete_many()
        .filter(job::Column::JobKind.eq(JobKind::DownloadAlbum))
        .filter(job::Column::DeduplicationKey.contains(format!("album:{}", album_id)))
        .exec(&state.db)
        .await?;

    // enqueue a new DL job for the album
    enqueue_download_album_job(state, album_id).await?;
    Ok(())
}
