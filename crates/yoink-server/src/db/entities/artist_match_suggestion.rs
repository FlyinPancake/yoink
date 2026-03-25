use sea_orm::{ActiveValue::Set, entity::prelude::*};
use uuid::Uuid;

use crate::db::{provider::Provider, url::DbUrl};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "artist_match_suggestions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub artist_id: Uuid,
    #[sea_orm(belongs_to, from = "artist_id", to = "id", on_delete = "Cascade")]
    pub artist: HasOne<super::artist::Entity>,
    pub provider: Provider,
    pub provider_artist_id: String,
    pub provider_artist_name: String,
    pub confidence: f32,
    pub url: Option<DbUrl>,
    pub image_url: Option<DbUrl>,
    pub created_at: DateTimeUtc,
    pub modified_at: DateTimeUtc,
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: Set(Uuid::now_v7()),
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
