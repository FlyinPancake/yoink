use std::collections::HashSet;

use crate::api::{SearchAlbumResult, SearchArtistResult, SearchTrackResult};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    db::{self, wanted_status::WantedStatus},
    error::AppResult,
    providers::{provider_image_url, registry::ProviderRegistry},
};

const SEARCH_ARTIST_IMAGE_SIZE: u16 = 320;

#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct SearchQuery {
    #[serde(deserialize_with = "serde_trim::string_trim")]
    pub query: String,
}

pub async fn search_aritsts(
    db: &DatabaseConnection,
    provider_registry: &ProviderRegistry,
    SearchQuery { query }: &SearchQuery,
) -> AppResult<Vec<SearchArtistResult>> {
    if query.is_empty() {
        return Ok(vec![]);
    }

    let artists: Vec<SearchArtistResult> = provider_registry
        .search_artists_all(query)
        .await
        .into_iter()
        .flat_map(|(provider, results)| {
            results.into_iter().map(move |result| SearchArtistResult {
                provider,
                external_id: result.external_id,
                name: result.name,
                image_url: result
                    .image_ref
                    .as_deref()
                    // Search result artist avatars render small in the UI, and
                    // some providers do not reliably serve larger variants for
                    // artist images.
                    .map(|r| provider_image_url(provider, r, SEARCH_ARTIST_IMAGE_SIZE)),
                url: result.url,
                disambiguation: result.disambiguation,
                artist_type: result.artist_type,
                country: result.country,
                tags: result.tags,
                popularity: result.popularity,
                already_monitored: None,
            })
        })
        .collect();

    let external_ids: Vec<String> = artists.iter().map(|a| a.external_id.clone()).collect();

    let has_artist: HashSet<String> = db::artist_provider_link::Entity::find()
        .select_only()
        .column(db::artist_provider_link::Column::ExternalId)
        .left_join(db::artist::Entity)
        .filter(db::artist::Column::Monitored.eq(true))
        .filter(db::artist_provider_link::Column::ExternalId.is_in(external_ids))
        .into_tuple::<String>()
        .all(db)
        .await?
        .into_iter()
        .collect();

    let artists = artists
        .into_iter()
        .map(|mut a| {
            if has_artist.contains(&a.external_id) {
                a.already_monitored = Some(true);
            } else {
                a.already_monitored = Some(false);
            }
            a
        })
        .collect();

    Ok(artists)
}

pub async fn search_albums(
    db: &DatabaseConnection,
    provider_registry: &ProviderRegistry,
    SearchQuery { query }: &SearchQuery,
) -> AppResult<Vec<SearchAlbumResult>> {
    let albums: Vec<_> = provider_registry
        .search_albums_all(query)
        .await
        .into_iter()
        .flat_map(|(provider, results)| {
            results.into_iter().map(move |result| SearchAlbumResult {
                provider,
                external_id: result.external_id,
                title: result.title,
                album_type: result
                    .album_type
                    .as_deref()
                    .map(db::album_type::AlbumType::parse),
                release_date: result.release_date,
                cover_url: result
                    .cover_ref
                    .as_deref()
                    .map(|r| provider_image_url(provider, r, 640)),
                url: result.url,
                explicit: result.explicit,
                artist_name: result.artist_name,
                artist_external_id: result.artist_external_id,
                already_added: None,
            })
        })
        .collect();

    tracing::info!("search_albums found {} results for `{query}`", albums.len());

    let already_added_ids: HashSet<String> = db::album_provider_link::Entity::find()
        .select_only()
        .column(db::album_provider_link::Column::ProviderAlbumId)
        .left_join(db::album::Entity)
        .filter(db::album::Column::WantedStatus.ne(WantedStatus::Unmonitored))
        .filter(
            db::album_provider_link::Column::ProviderAlbumId.is_in(
                albums
                    .iter()
                    .map(|a| a.external_id.clone())
                    .collect::<Vec<_>>(),
            ),
        )
        .into_tuple::<String>()
        .all(db)
        .await?
        .into_iter()
        .collect();

    let albums = albums
        .into_iter()
        .map(|mut a| {
            if already_added_ids.contains(&a.external_id) {
                a.already_added = Some(true);
            } else {
                a.already_added = Some(false);
            }
            a
        })
        .collect();

    Ok(albums)
}

pub async fn search_tracks(
    db: &DatabaseConnection,
    provider_registry: &ProviderRegistry,
    SearchQuery { query }: &SearchQuery,
) -> AppResult<Vec<SearchTrackResult>> {
    let tracks: Vec<_> = provider_registry
        .search_tracks_all(query)
        .await
        .into_iter()
        .flat_map(|(provider, results)| {
            results.into_iter().map(move |result| SearchTrackResult {
                provider,
                external_id: result.external_id,
                title: result.title,
                explicit: result.explicit,
                artist_name: result.artist_name,
                artist_external_id: result.artist_external_id,
                album_title: result.album_title,
                album_external_id: result.album_external_id,
                version: result.version,
                duration_secs: result.duration_secs,
                isrc: result.isrc,
                album_cover_url: result
                    .album_cover_ref
                    .as_deref()
                    .map(|r| provider_image_url(provider, r, 640)),
                already_added: None,
            })
        })
        .collect();

    tracing::info!("search_tracks found {} results for `{query}`", tracks.len());

    let already_added_ids: HashSet<String> = db::track_provider_link::Entity::find()
        .select_only()
        .column(db::track_provider_link::Column::ProviderTrackId)
        .left_join(db::track::Entity)
        .filter(db::track::Column::Status.ne(WantedStatus::Unmonitored))
        .filter(
            db::track_provider_link::Column::ProviderTrackId.is_in(
                tracks
                    .iter()
                    .map(|t| t.external_id.clone())
                    .collect::<Vec<_>>(),
            ),
        )
        .into_tuple::<String>()
        .all(db)
        .await?
        .into_iter()
        .collect();

    let tracks = tracks
        .into_iter()
        .map(|mut t| {
            if already_added_ids.contains(&t.external_id) {
                t.already_added = Some(true);
            } else {
                t.already_added = Some(false);
            }
            t
        })
        .collect();

    Ok(tracks)
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchAllResult {
    pub artists: Vec<SearchArtistResult>,
    pub albums: Vec<SearchAlbumResult>,
    pub tracks: Vec<SearchTrackResult>,
}

pub async fn search_all(
    db: &DatabaseConnection,
    provider_registry: &ProviderRegistry,
    query: &SearchQuery,
) -> AppResult<SearchAllResult> {
    tracing::warn!("search_all is currently stubbed out");
    Ok(SearchAllResult {
        artists: search_aritsts(db, provider_registry, query).await?,
        albums: search_albums(db, provider_registry, query).await?,
        tracks: search_tracks(db, provider_registry, query).await?,
    })
}
