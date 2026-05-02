use async_trait::async_trait;
use sea_orm::{ActiveValue::Set, entity::prelude::*};

use crate::{
    db::{job_kind::JobKind, job_status::JobStatus},
    services::jobs::Job,
};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "jobs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: uuid::Uuid,
    pub job_kind: JobKind,
    pub data: Job,
    pub status: JobStatus,
    pub deduplication_key: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub progress: f32,
    pub error_message: Option<String>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub modified_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: Set(uuid::Uuid::now_v7()),
            status: Set(JobStatus::Queued),
            attempts: Set(0),
            progress: Set(0.0),
            ..ActiveModelTrait::default()
        }
    }

    /// Will be triggered before insert / update
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let now = chrono::Utc::now();
        self.modified_at = Set(now);
        if let Set(data) = &self.data {
            self.job_kind = Set(data.kind());
        }
        if insert {
            self.created_at = Set(now);
        }
        Ok(self)
    }
}
