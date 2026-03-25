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
    pub source: Provider,
    pub quality: Quality,
    pub status: DownloadStatus,
    pub total_tracks: i32,
    pub completed_tasks: i32,
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

impl From<ModelEx> for yoink_shared::DownloadJob {
    fn from(value: ModelEx) -> Self {
        let album_title = value
            .album
            .as_ref()
            .map(|a| a.title.clone())
            .unwrap_or_default();

        Self {
            id: value.id,
            album_id: value.album_id,
            source: value.source.to_value(),
            quality: value.quality.into(),
            status: value.status.into(),
            total_tracks: value.total_tracks,
            created_at: value.created_at,
            album_title,
            updated_at: value.modified_at,
            completed_tracks: value.completed_tasks,
            // FIXME remove or fill these placeholders
            artist_name: "".to_string(),
            error: Some("".to_string()),
        }
    }
}
