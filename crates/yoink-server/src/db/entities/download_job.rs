use sea_orm::{ActiveValue::Set, entity::prelude::*};

use crate::db::{download_status::DownloadStatus, provider::Provider, quality::Quality};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "download_jobs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: uuid::Uuid,
    pub album_id: uuid::Uuid,
    #[sea_orm(belongs_to, from = "album_id", to = "id", on_delete = "Cascade")]
    pub album: HasOne<super::album::Entity>,
    pub track_id: Option<uuid::Uuid>,
    #[sea_orm(belongs_to, from = "track_id", to = "id", on_delete = "SetNull")]
    pub track: Option<super::track::Entity>,
    pub source: Provider,
    pub quality: Quality,
    pub status: DownloadStatus,
    pub total_tracks: i32,
    pub completed_tasks: i32,
    pub error_message: Option<String>,
    pub created_at: DateTimeUtc,
    pub modified_at: DateTimeUtc,
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: Set(uuid::Uuid::now_v7()),
            status: Set(DownloadStatus::Queued),
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
        if insert {
            self.created_at = Set(now);
        }
        Ok(self)
    }
}
