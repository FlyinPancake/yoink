use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    db::{self, job_status::JobStatus},
    services::jobs::Job,
};

#[derive(Debug, Clone, utoipa::ToSchema, Serialize, Deserialize)]
pub struct JobResponse {
    #[serde(flatten)]
    pub payload: Job,
    pub id: Uuid,
    pub status: JobStatus,
    pub progress: f32,
    pub attempts: i32,
    pub max_attempts: i32,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl From<db::job::Model> for JobResponse {
    fn from(value: db::job::Model) -> Self {
        Self {
            id: value.id,
            status: value.status,
            progress: value.progress,
            attempts: value.attempts,
            max_attempts: value.max_attempts,
            error_message: value.error_message,
            created_at: value.created_at,
            modified_at: value.modified_at,
            started_at: value.started_at,
            finished_at: value.finished_at,
            payload: value.data,
        }
    }
}
