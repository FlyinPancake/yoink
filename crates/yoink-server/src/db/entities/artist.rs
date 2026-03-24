use async_trait::async_trait;
use sea_orm::{ActiveValue::Set, entity::prelude::*};
use uuid::Uuid;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "artists")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub name: String,
    pub image_url: Option<String>,
    pub bio: Option<String>,
    pub monitored: bool,
    #[sea_orm(has_many, via = "album_artist")]
    pub albums: HasMany<super::album::Entity>,
    #[sea_orm(has_many)]
    pub provider_links: HasMany<super::artist_provider_link::Entity>,
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

impl From<Model> for yoink_shared::MonitoredArtist {
    fn from(value: Model) -> Self {
        Self {
            id: value.id,
            name: value.name,
            image_url: value.image_url,
            bio: value.bio,
            monitored: value.monitored,
            created_at: value.created_at,
        }
    }
}

impl From<ModelEx> for yoink_shared::MonitoredArtist {
    fn from(value: ModelEx) -> Self {
        Model::from(value).into()
    }
}
