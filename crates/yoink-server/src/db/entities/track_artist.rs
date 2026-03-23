use sea_orm::entity::prelude::*;
use uuid::Uuid;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "track_artists")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub track_id: Uuid,
    #[sea_orm(primary_key)]
    pub artist_id: Uuid,
    #[sea_orm(belongs_to, from = "track_id", to = "id")]
    pub track: Option<super::track::Entity>,
    #[sea_orm(belongs_to, from = "artist_id", to = "id")]
    pub artist: Option<super::artist::Entity>,
    #[sea_orm(default_value = "0")]
    pub priority: i32,
}

impl ActiveModelBehavior for ActiveModel {}
