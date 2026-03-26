use sea_orm::{ActiveValue::Set, DeleteMany, QueryOrder, entity::prelude::*};
use uuid::Uuid;

use crate::db::provider::Provider;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "album_match_candidates")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub album_id: Uuid,
    #[sea_orm(belongs_to, from = "album_id", to = "id", on_delete = "Cascade")]
    pub album: HasOne<super::album::Entity>,
    pub left_provider: Provider,
    pub left_external_id: String,
    pub right_provider: Provider,
    pub right_external_id: String,
    pub match_kind: String,
    pub confidence: i32,
    pub explanation: Option<String>,
    pub external_name: Option<String>,
    pub external_url: Option<String>,
    pub image_url: Option<String>,
    pub tags_json: Option<String>,
    pub popularity: Option<i32>,
    pub status: String,
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

impl Entity {
    pub fn find_by_album(album_id: Uuid) -> Select<Entity> {
        Entity::find()
            .filter(Column::AlbumId.eq(album_id))
            .order_by_asc(Column::Status)
            .order_by_desc(Column::Confidence)
            .order_by_desc(Column::CreatedAt)
    }

    pub fn delete_pending_for_album(album_id: Uuid) -> DeleteMany<Entity> {
        Entity::delete_many()
            .filter(Column::AlbumId.eq(album_id))
            .filter(Column::Status.eq("pending"))
    }
}
