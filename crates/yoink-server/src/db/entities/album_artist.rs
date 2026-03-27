use sea_orm::{QueryFilter, QueryOrder, entity::prelude::*};
use uuid::Uuid;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "album_artists")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub album_id: Uuid,
    #[sea_orm(primary_key)]
    pub artist_id: Uuid,
    #[sea_orm(belongs_to, from = "album_id", to = "id", on_delete = "Cascade")]
    pub album: Option<super::album::Entity>,
    #[sea_orm(belongs_to, from = "artist_id", to = "id", on_delete = "Cascade")]
    pub artist: Option<super::artist::Entity>,
    #[sea_orm(default_value = "0")]
    pub priority: i32,
}

impl ActiveModelBehavior for ActiveModel {}

impl Entity {
    /// Find all junction records for an artist.
    pub fn find_by_artist(artist_id: Uuid) -> Select<Entity> {
        Entity::find().filter(Column::ArtistId.eq(artist_id))
    }

    /// Find all junction records for an album, ordered by priority.
    pub fn find_by_album_ordered(album_id: Uuid) -> Select<Entity> {
        Entity::find()
            .filter(Column::AlbumId.eq(album_id))
            .order_by_asc(Column::Priority)
    }

    /// Find all junction records for a set of albums, ordered by album then priority.
    pub fn find_by_album_ids_ordered<I>(album_ids: I) -> Select<Entity>
    where
        I: IntoIterator<Item = Uuid>,
    {
        let album_ids: Vec<Uuid> = album_ids.into_iter().collect();
        Entity::find()
            .filter(Column::AlbumId.is_in(album_ids))
            .order_by_asc(Column::AlbumId)
            .order_by_asc(Column::Priority)
    }

    /// Delete a specific (album_id, artist_id) junction record.
    pub fn delete_pair(album_id: Uuid, artist_id: Uuid) -> sea_orm::DeleteMany<Entity> {
        Entity::delete_many()
            .filter(Column::AlbumId.eq(album_id))
            .filter(Column::ArtistId.eq(artist_id))
    }
}
