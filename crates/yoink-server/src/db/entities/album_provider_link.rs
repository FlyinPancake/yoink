use sea_orm::{ActiveValue::Set, entity::prelude::*};

use crate::db::provider::Provider;
use crate::services::helpers::default_provider_album_url;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "album_provider_links")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: uuid::Uuid,
    pub album_id: uuid::Uuid,
    #[sea_orm(belongs_to, from = "album_id", to = "id", on_delete = "Cascade")]
    pub album: Option<super::album::Entity>,
    pub provider: Provider,
    pub provider_album_id: String,
    pub external_url: Option<String>,
    pub external_name: Option<String>,
    pub created_at: DateTimeUtc,
    pub modified_at: DateTimeUtc,
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: Set(uuid::Uuid::now_v7()),
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

impl From<Model> for yoink_shared::ProviderLink {
    fn from(value: Model) -> Self {
        Self {
            provider: value.provider.to_value(),
            external_url: value.external_url.or_else(|| {
                default_provider_album_url(&value.provider.to_value(), &value.provider_album_id)
            }),
            external_name: value.external_name,
            external_id: value.provider_album_id,
        }
    }
}
