use async_trait::async_trait;
use sea_orm::{ActiveValue::Set, QueryOrder, entity::prelude::*};

use crate::db::{entities::album_type::AlbumType, quality::Quality, wanted_status::WantedStatus};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "albums")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    #[sea_orm(has_many, via = "album_artist")]
    pub artists: HasMany<super::artist::Entity>,
    #[sea_orm(has_many)]
    pub tracks: HasMany<super::track::Entity>,
    #[sea_orm(has_many)]
    pub download_jobs: HasMany<super::download_job::Entity>,
    #[sea_orm(has_many)]
    pub provider_links: HasMany<super::album_provider_link::Entity>,
    pub title: String,
    pub album_type: AlbumType,
    pub release_date: Option<chrono::NaiveDate>,
    pub cover_url: Option<String>,
    pub explicit: bool,
    pub wanted_status: WantedStatus,
    pub requested_quality: Option<Quality>,
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

impl From<Model> for yoink_shared::Album {
    fn from(value: Model) -> Self {
        Self {
            id: value.id,
            title: value.title,
            album_type: Some(value.album_type.to_value()),
            release_date: value.release_date.map(|d| d.to_string()),
            cover_url: value.cover_url.map(|u| u.to_string()),
            explicit: value.explicit,
            monitored: value.wanted_status != WantedStatus::Unmonitored,
            wanted_status: value.wanted_status.into(),
            quality_override: value.requested_quality.map(Into::into),
            created_at: value.created_at,
        }
    }
}

impl From<ModelEx> for yoink_shared::Album {
    fn from(value: ModelEx) -> Self {
        Model::from(value).into()
    }
}

impl From<WantedStatus> for yoink_shared::WantedStatus {
    fn from(value: WantedStatus) -> Self {
        match value {
            WantedStatus::Unmonitored => Self::Unwanted,
            WantedStatus::Wanted => Self::Wanted,
            WantedStatus::InProgress => Self::InProgress,
            WantedStatus::Acquired => Self::Acquired,
        }
    }
}

impl ModelEx {
    pub async fn fetch_primary_artist<C>(
        &self,
        db: &C,
    ) -> Result<Option<super::artist::Model>, DbErr>
    where
        C: ConnectionTrait,
    {
        match super::album_artist::Entity::find()
            .filter(super::album_artist::Column::AlbumId.eq(self.id))
            .find_also_related(super::artist::Entity)
            .order_by_asc(super::album_artist::Column::Priority)
            .one(db)
            .await?
        {
            Some((_, Some(artist))) => Ok(Some(artist)),
            _ => Ok(None),
        }
    }
}
