use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use sea_orm::{ColumnTrait, EntityLoaderTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use url::Url;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use yoink_shared::{Album, ArtistImageOption, MonitoredArtist, ProviderLink, SearchArtistResult};

use crate::{
    db::{self, provider::Provider, quality::Quality},
    error::AppError,
    services::{self, AlbumMatchSuggestion, ArtistMatchSuggestion, search::SearchQuery},
    state::AppState,
};

use super::helpers::{ApiErrorResponse, app_error_response};

pub(crate) const TAG: &str = "Artist";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for artist search, lookup, and lifecycle";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;
type ApiStatusResult = Result<StatusCode, ApiErrorResponse>;

#[derive(Debug, Deserialize, ToSchema)]
struct DeleteArtistQuery {
    #[serde(default)]
    remove_files: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
struct CreateArtistRequest {
    name: String,
    provider: Provider,
    external_id: String,
    #[serde(default)]
    image_url: Option<String>,
    #[serde(default)]
    external_url: Option<Url>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct UpdateArtistRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    image_url: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct ToggleArtistMonitorRequest {
    monitored: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
struct LinkArtistProviderRequest {
    provider: Provider,
    external_id: String,
    #[serde(default)]
    external_url: Option<String>,
    #[serde(default)]
    external_name: Option<String>,
    #[serde(default)]
    image_ref: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
struct UnlinkArtistProviderRequest {
    provider: String,
    external_id: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
struct ArtistDetailResponse {
    artist: MonitoredArtist,
    albums: Vec<Album>,
    provider_links: Vec<ProviderLink>,
    artist_match_suggestions: Vec<ArtistMatchSuggestion>,
    album_match_suggestions: Vec<AlbumMatchSuggestion>,
    default_quality: Quality,
}

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(search_artists))
        .routes(routes!(list_artists))
        .routes(routes!(create_artist))
        .routes(routes!(get_artist))
        .routes(routes!(update_artist))
        .routes(routes!(delete_artist))
        .routes(routes!(toggle_artist_monitor))
        .routes(routes!(sync_artist))
        .routes(routes!(fetch_artist_bio))
        .routes(routes!(list_artist_providers))
        .routes(routes!(link_artist_provider))
        .routes(routes!(unlink_artist_provider))
        .routes(routes!(get_artist_images))
        .routes(routes!(refresh_match_suggestions))
}

#[utoipa::path(
    get,
    path = "/search",
    tag = TAG,
    params(
        ("query" = String, Query, description = "Artist search query")
    ),
    responses(
        (status = 200, description = "Search results across all providers", body = Vec<SearchArtistResult>),
        (status = 503, description = "Provider search unavailable"),
    )
)]
/// Search Artists
///
/// Searches all registered metadata providers for artists matching the query
/// string and returns the aggregated results.
async fn search_artists(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Vec<SearchArtistResult>> {
    services::search::search_aritsts(&state.db, &state.registry, &query)
        .await
        .map_err(app_error_response)
        .map(Json)
}

#[utoipa::path(
    get,
    path = "/",
    tag = TAG,
    responses(
        (status = 200, description = "All local artists", body = Vec<MonitoredArtist>),
        (status = 500, description = "Failed to load artists"),
    )
)]
/// List Artists
///
/// Returns all locally monitored artists from the library database.
async fn list_artists(State(state): State<AppState>) -> ApiResult<Vec<MonitoredArtist>> {
    let artists = db::artist::Entity::find()
        .all(&state.db)
        .await
        .map(|models| models.into_iter().map(Into::into).collect())
        .map_err(|e| app_error_response(e.into()))?;
    Ok(Json(artists))
}

#[utoipa::path(
    post,
    path = "/",
    tag = TAG,
    request_body = CreateArtistRequest,
    responses(
        (status = 201, description = "Artist created"),
        (status = 409, description = "Artist already exists"),
        (status = 500, description = "Failed to create artist"),
    )
)]
/// Create Artist
///
/// Adds an artist from provider metadata, persists the provider link, and
/// triggers the existing artist sync flow.
async fn create_artist(
    State(state): State<AppState>,
    Json(request): Json<CreateArtistRequest>,
) -> ApiStatusResult {
    services::artist::add_artist(
        &state,
        request.name,
        request.provider,
        request.external_id,
        request.image_url,
        request.external_url,
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::CREATED)
}

#[utoipa::path(
    get,
    path = "/{artist_id}",
    tag = TAG,
    params(
        ("artist_id" = Uuid, Path, description = "Artist UUID")
    ),
    responses(
        (status = 200, description = "Artist detail payload", body = ArtistDetailResponse),
        (status = 404, description = "Artist not found"),
        (status = 500, description = "Failed to load artist detail"),
    )
)]
/// Get Artist
///
/// Returns the local artist, related albums, provider links, and match
/// suggestions for a single artist.
async fn get_artist(
    State(state): State<AppState>,
    Path(artist_id): Path<Uuid>,
) -> ApiResult<ArtistDetailResponse> {
    let db::artist::ModelEx {
        id,
        name,
        image_url,
        bio,
        monitored,
        created_at,
        match_suggestions,
        modified_at: _,
        albums: loaded_albums,
        provider_links: loaded_links,
    } = db::artist::Entity::load()
        .filter_by_id(artist_id)
        .with(db::album::Entity)
        .with(db::artist_provider_link::Entity)
        .with(db::artist_match_suggestion::Entity)
        .one(&state.db)
        .await
        .map_err(|e| app_error_response(e.into()))?
        .ok_or_else(|| {
            app_error_response(AppError::not_found("artist", Some(artist_id.to_string())))
        })?;

    let artist = MonitoredArtist {
        id,
        name,
        image_url,
        bio,
        monitored,
        created_at,
    };
    let album_ids: Vec<Uuid> = loaded_albums.iter().map(|album| album.id).collect();
    let albums: Vec<Album> = loaded_albums.into_iter().map(Into::into).collect();
    let provider_links: Vec<ProviderLink> = loaded_links.into_iter().map(Into::into).collect();

    let artist_match_suggestions: Vec<ArtistMatchSuggestion> =
        match_suggestions.into_iter().map(Into::into).collect();

    let album_match_suggestions: Vec<AlbumMatchSuggestion> = if album_ids.is_empty() {
        Vec::new()
    } else {
        db::album_match_suggestion::Entity::find()
            .filter(db::album_match_suggestion::Column::AlbumId.is_in(album_ids))
            .order_by_asc(db::album_match_suggestion::Column::Status)
            .order_by_desc(db::album_match_suggestion::Column::Confidence)
            .order_by_desc(db::album_match_suggestion::Column::CreatedAt)
            .all(&state.db)
            .await
            .map(|models| models.into_iter().map(Into::into).collect())
            .map_err(|e| app_error_response(e.into()))?
    };

    Ok(Json(ArtistDetailResponse {
        artist,
        albums,
        provider_links,
        artist_match_suggestions,
        album_match_suggestions,
        default_quality: state.default_quality,
    }))
}

#[utoipa::path(
    patch,
    path = "/{artist_id}",
    tag = TAG,
    params(
        ("artist_id" = Uuid, Path, description = "Artist UUID")
    ),
    request_body = UpdateArtistRequest,
    responses(
        (status = 204, description = "Artist updated"),
        (status = 404, description = "Artist not found"),
        (status = 500, description = "Failed to update artist"),
    )
)]
/// Update Artist
///
/// Updates editable local artist fields such as name and image URL.
async fn update_artist(
    State(state): State<AppState>,
    Path(artist_id): Path<Uuid>,
    Json(request): Json<UpdateArtistRequest>,
) -> ApiStatusResult {
    services::artist::update_artist(&state, artist_id, request.name, request.image_url)
        .await
        .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/{artist_id}",
    tag = TAG,
    params(
        ("artist_id" = Uuid, Path, description = "Artist UUID"),
        ("remove_files" = Option<bool>, Query, description = "Whether to remove downloaded files")
    ),
    responses(
        (status = 204, description = "Artist removed"),
        (status = 404, description = "Artist not found"),
        (status = 500, description = "Failed to remove artist"),
    )
)]
/// Delete Artist
///
/// Removes an artist from the local library and optionally removes downloaded
/// files associated with that artist.
async fn delete_artist(
    State(state): State<AppState>,
    Path(artist_id): Path<Uuid>,
    Query(query): Query<DeleteArtistQuery>,
) -> ApiStatusResult {
    services::artist::remove_artist(&state, artist_id, query.remove_files)
        .await
        .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    patch,
    path = "/{artist_id}/monitor",
    tag = TAG,
    params(
        ("artist_id" = Uuid, Path, description = "Artist UUID")
    ),
    request_body = ToggleArtistMonitorRequest,
    responses(
        (status = 204, description = "Artist monitor flag updated"),
        (status = 404, description = "Artist not found"),
        (status = 500, description = "Failed to toggle artist monitor"),
    )
)]
/// Toggle Artist Monitor
///
/// Enables or disables full monitoring for an artist.
async fn toggle_artist_monitor(
    State(state): State<AppState>,
    Path(artist_id): Path<Uuid>,
    Json(request): Json<ToggleArtistMonitorRequest>,
) -> ApiStatusResult {
    services::artist::toggle_artist_monitor(&state, artist_id, request.monitored)
        .await
        .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/{artist_id}/sync",
    tag = TAG,
    params(
        ("artist_id" = Uuid, Path, description = "Artist UUID")
    ),
    responses(
        (status = 204, description = "Artist albums synced"),
        (status = 404, description = "Artist not found"),
        (status = 500, description = "Failed to sync artist albums"),
    )
)]
/// Sync Artist
///
/// Triggers a discography sync for the specified artist using its linked
/// provider metadata.
async fn sync_artist(
    State(state): State<AppState>,
    Path(artist_id): Path<Uuid>,
) -> ApiStatusResult {
    services::artist::sync_artist_and_refresh(&state, artist_id)
        .await
        .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/{artist_id}/fetch-bio",
    tag = TAG,
    params(
        ("artist_id" = Uuid, Path, description = "Artist UUID")
    ),
    responses(
        (status = 204, description = "Artist bio refresh triggered"),
        (status = 404, description = "Artist not found"),
        (status = 500, description = "Failed to refresh artist bio"),
    )
)]
/// Fetch Artist Bio
///
/// Starts a background refresh of the artist biography from linked providers.
async fn fetch_artist_bio(
    State(state): State<AppState>,
    Path(artist_id): Path<Uuid>,
) -> ApiStatusResult {
    services::artist::fetch_artist_bio(&state, artist_id)
        .await
        .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/{artist_id}/provider",
    tag = TAG,
    params(
        ("artist_id" = Uuid, Path, description = "Artist UUID")
    ),
    responses(
        (status = 200, description = "Provider links for the artist", body = Vec<ProviderLink>),
        (status = 404, description = "Artist not found"),
        (status = 500, description = "Failed to load provider links"),
    )
)]
/// List Artist Providers
///
/// Returns the provider links currently attached to a local artist.
async fn list_artist_providers(
    State(state): State<AppState>,
    Path(artist_id): Path<Uuid>,
) -> ApiResult<Vec<ProviderLink>> {
    let links = db::artist_provider_link::Entity::find_by_artist(artist_id)
        .all(&state.db)
        .await
        .map_err(|e| app_error_response(e.into()))?
        .into_iter()
        .map(Into::into)
        .collect();

    Ok(Json(links))
}

#[utoipa::path(
    post,
    path = "/{artist_id}/provider",
    tag = TAG,
    params(
        ("artist_id" = Uuid, Path, description = "Artist UUID")
    ),
    request_body = LinkArtistProviderRequest,
    responses(
        (status = 204, description = "Provider linked to artist"),
        (status = 404, description = "Artist not found"),
        (status = 500, description = "Failed to link provider"),
    )
)]
/// Link Artist Provider
///
/// Attaches a provider identity to an existing local artist.
async fn link_artist_provider(
    State(state): State<AppState>,
    Path(artist_id): Path<Uuid>,
    Json(request): Json<LinkArtistProviderRequest>,
) -> ApiStatusResult {
    services::artist::link_artist_provider(
        &state,
        artist_id,
        request.provider,
        request.external_id,
        request.external_url,
        request.external_name,
        request.image_ref,
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/{artist_id}/provider",
    tag = TAG,
    params(
        ("artist_id" = Uuid, Path, description = "Artist UUID")
    ),
    request_body = UnlinkArtistProviderRequest,
    responses(
        (status = 204, description = "Provider unlinked from artist"),
        (status = 404, description = "Artist not found"),
        (status = 500, description = "Failed to unlink provider"),
    )
)]
/// Unlink Artist Provider
///
/// Removes a provider identity from an existing local artist.
async fn unlink_artist_provider(
    State(state): State<AppState>,
    Path(artist_id): Path<Uuid>,
    Json(request): Json<UnlinkArtistProviderRequest>,
) -> ApiStatusResult {
    services::artist::unlink_artist_provider(
        &state,
        artist_id,
        request.provider,
        request.external_id,
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/{artist_id}/image",
    tag = TAG,
    params(
        ("artist_id" = Uuid, Path, description = "Artist UUID")
    ),
    responses(
        (status = 200, description = "Available artist images from linked providers", body = Vec<ArtistImageOption>),
        (status = 404, description = "Artist not found"),
        (status = 500, description = "Failed to fetch artist images"),
    )
)]
/// Get Artist Images
///
/// Returns candidate artist images collected from the artist's linked
/// providers.
async fn get_artist_images(
    State(state): State<AppState>,
    Path(artist_id): Path<Uuid>,
) -> ApiResult<Vec<ArtistImageOption>> {
    services::artist::get_artist_images(&state, artist_id)
        .await
        .map_err(app_error_response)
        .map(Json)
}

#[utoipa::path(
    post,
    path = "/{artist_id}/match-suggestion/refresh",
    tag = TAG,
    params(
        ("artist_id" = Uuid, Path, description = "Artist UUID")
    ),
    responses(
        (status = 204, description = "Artist match suggestions refreshed"),
        (status = 404, description = "Artist not found"),
        (status = 500, description = "Failed to refresh match suggestions"),
    )
)]
/// Refresh Match Suggestions
///
/// Recomputes pending artist match suggestions for the specified artist.
async fn refresh_match_suggestions(
    State(state): State<AppState>,
    Path(artist_id): Path<Uuid>,
) -> ApiStatusResult {
    services::recompute_artist_match_suggestions(&state, artist_id)
        .await
        .map_err(app_error_response)?;
    state.notify_sse();

    Ok(StatusCode::NO_CONTENT)
}
