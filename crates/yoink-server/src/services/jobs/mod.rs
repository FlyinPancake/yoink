pub mod download;
pub mod metadata;

use sea_orm::{
    ActiveEnum, ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    FromJsonQueryResult, IntoActiveModel, QueryFilter,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    db::{job, job_status::JobStatus},
    error::{AppError, AppResult},
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

    if let Some(existing) = job::Entity::find()
        .filter(job::Column::DeduplicationKey.eq(&deduplication_key))
        .one(&state.db)
        .await?
    {
        let job = match existing.status {
            JobStatus::Running => {
                tracing::warn!("job already running, not touching it");
                existing
            }
            JobStatus::Queued | JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled => {
                let mut active = existing.into_active_model();
                active.data = Set(job);
                active.status = Set(JobStatus::Queued);
                active.attempts = Set(0);
                active.max_attempts = Set(MAX_ATTEMPTS);
                active.progress = Set(0.0);
                active.error_message = Set(None);
                active.started_at = Set(None);
                active.finished_at = Set(None);
                active.update(&state.db).await?
            }
        };

        state.download_notify.notify_one();

        return Ok(job);
    }

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

pub(crate) async fn clear_completed_jobs(db: &DatabaseConnection) -> AppResult<()> {
    job::Entity::delete_many()
        .filter(job::Column::Status.eq(JobStatus::Succeeded))
        .exec(db)
        .await?;
    Ok(())
}

pub(crate) async fn cancel_job(state: &AppState, job_id: Uuid) -> AppResult<()> {
    let job = job::Entity::find_by_id(job_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::not_found("job", Some(job_id)))?;

    match job.status {
        JobStatus::Queued => {
            job.into_ex()
                .into_active_model()
                .set_status(JobStatus::Cancelled)
                .update(&state.db)
                .await?;
        }
        _ => {
            return Err(AppError::validation(
                Some("job"),
                format!("cannot cancel a job in this status: {:?}", job.status),
            ));
        }
    }

    Ok(())
}

pub(crate) async fn list_jobs_for_album<C>(db: &C, album_id: Uuid) -> AppResult<Vec<job::Model>>
where
    C: sea_orm::ConnectionTrait,
{
    let jobs = job::Entity::find()
        .filter(job::Column::DeduplicationKey.contains(format!("album:{}", album_id)))
        .all(db)
        .await?;

    Ok(jobs)
}

pub(crate) async fn list_jobs_for_track<C>(db: &C, track_id: Uuid) -> AppResult<Vec<job::Model>>
where
    C: sea_orm::ConnectionTrait,
{
    let jobs = job::Entity::find()
        .filter(job::Column::DeduplicationKey.contains(format!("track:{}", track_id)))
        .all(db)
        .await?;

    Ok(jobs)
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

pub(crate) async fn prepare_album_for_unmonitor(state: &AppState, album_id: Uuid) -> AppResult<()> {
    let jobs = list_jobs_for_album(&state.db, album_id).await?;
    if jobs.iter().any(|job| job.status == JobStatus::Running) {
        return Err(AppError::conflict(
            "cannot unmonitor album while a download job is running",
        ));
    }

    for job in jobs {
        if matches!(job.status, JobStatus::Queued) {
            cancel_job(state, job.id).await?;
        }
    }
    Ok(())
}

pub(crate) async fn prepare_track_for_unmonitor(state: &AppState, track_id: Uuid) -> AppResult<()> {
    let jobs = list_jobs_for_track(&state.db, track_id).await?;
    if jobs.iter().any(|job| job.status == JobStatus::Running) {
        return Err(AppError::conflict(
            "cannot unmonitor track while a download job is running",
        ));
    }

    for job in jobs {
        if matches!(job.status, JobStatus::Queued | JobStatus::Failed) {
            cancel_job(state, job.id).await?;
        }
    }
    Ok(())
}
