use sea_orm::{ActiveValue::Set, entity::prelude::*};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "artist_provider_links")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub artist_id: Uuid,
    #[sea_orm(belongs_to, from = "artist_id", to = "id", on_delete = "Cascade")]
    pub artist: HasOne<super::artist::Entity>,
    pub provider: super::provider::Provider,
    #[sea_orm(indexed)]
    pub external_id: String,
    pub external_url: Option<String>,
    pub external_name: Option<String>,
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

use super::provider::Provider;

impl Entity {
    /// Find all provider links for an artist.
    pub fn find_by_artist(artist_id: Uuid) -> Select<Entity> {
        Entity::find().filter(Column::ArtistId.eq(artist_id))
    }

    /// Find a provider link by (provider, external_id).
    pub fn find_by_provider_external(provider: Provider, external_id: &str) -> Select<Entity> {
        Entity::find()
            .filter(Column::Provider.eq(provider))
            .filter(Column::ExternalId.eq(external_id))
    }

    /// Find a provider link by (artist_id, provider, external_id).
    pub fn find_by_artist_provider_external(
        artist_id: Uuid,
        provider: Provider,
        external_id: &str,
    ) -> Select<Entity> {
        Entity::find()
            .filter(Column::ArtistId.eq(artist_id))
            .filter(Column::Provider.eq(provider))
            .filter(Column::ExternalId.eq(external_id))
    }

    /// Delete a provider link by (artist_id, provider, external_id).
    pub fn delete_by_artist_provider_external(
        artist_id: Uuid,
        provider: Provider,
        external_id: &str,
    ) -> sea_orm::DeleteMany<Entity> {
        Entity::delete_many()
            .filter(Column::ArtistId.eq(artist_id))
            .filter(Column::Provider.eq(provider))
            .filter(Column::ExternalId.eq(external_id.to_string()))
    }
}
