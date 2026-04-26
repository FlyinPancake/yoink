pub mod download;
pub mod metadata;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, FromJsonQueryResult};
use serde::{Deserialize, Serialize};

use crate::{db::job, error::AppResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromJsonQueryResult)]
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
        let prefix =
            serde_json::to_string(&self.kind()).unwrap_or_else(|_| String::from("unknown"));
        let suffix = match self {
            Job::DownloadAlbum { payload } => format!("album:{}", payload.album_id),
            Job::DownloadTrack { payload } => format!("track:{}", payload.track_id),
        };
        format!("{}:{}", prefix, suffix)
    }
}

const MAX_ATTEMPTS: i32 = 3;

pub(crate) async fn enqueue_job(db: &DatabaseConnection, job: Job) -> AppResult<job::Model> {
    let deduplication_key = job.dedupe_key();
    let job = job::ActiveModel {
        data: Set(job),
        deduplication_key: Set(deduplication_key),
        max_attempts: Set(MAX_ATTEMPTS),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(job)
}
