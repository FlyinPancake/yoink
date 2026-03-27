use async_trait::async_trait;
use sea_orm::{ActiveValue::Set, QueryFilter, QueryOrder, entity::prelude::*};

use crate::db::quality::Quality;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "tracks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub title: String,
    /// XY remix, remaster, etc.
    pub version: Option<String>,
    pub disc_number: Option<i32>,
    pub track_number: Option<i32>,
    pub duration: Option<i32>,
    pub album_id: Uuid,
    #[sea_orm(belongs_to, from = "album_id", to = "id", on_delete = "Cascade")]
    pub album: HasOne<super::album::Entity>,
    #[sea_orm(has_many, via = "track_artist")]
    pub artists: HasMany<super::artist::Entity>,
    pub explicit: bool,
    /// International Standard Recording Code
    pub isrc: Option<String>,
    pub root_folder_id: Option<Uuid>,
    #[sea_orm(belongs_to, from = "root_folder_id", to = "id", on_delete = "SetNull")]
    pub root_folder: Option<super::root_folder::Entity>,
    pub status: super::wanted_status::WantedStatus,
    pub quality_override: Option<Quality>,
    pub file_path: Option<String>,
    pub created_at: DateTimeUtc,
    pub modified_at: DateTimeUtc,
}

#[async_trait]
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

impl Entity {
    pub fn find_by_album_ids_ordered<I>(album_ids: I) -> Select<Entity>
    where
        I: IntoIterator<Item = Uuid>,
    {
        let album_ids: Vec<Uuid> = album_ids.into_iter().collect();
        Entity::find()
            .filter(Column::AlbumId.is_in(album_ids))
            .order_by_asc(Column::AlbumId)
            .order_by_asc(Column::DiscNumber)
            .order_by_asc(Column::TrackNumber)
            .order_by_asc(Column::Title)
    }
}
