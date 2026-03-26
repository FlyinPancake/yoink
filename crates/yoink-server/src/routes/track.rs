use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use yoink_shared::{LibraryTrack, SearchTrackResult};

use crate::{
    db::provider::Provider,
    services::{self, search::SearchQuery},
    state::AppState,
};

use super::helpers::{ApiErrorResponse, app_error_response};

pub(crate) const TAG: &str = "Track";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for track search and library track access";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;
type ApiStatusResult = Result<StatusCode, ApiErrorResponse>;

#[derive(Debug, Deserialize, ToSchema)]
struct CreateTrackRequest {
    provider: Provider,
    external_track_id: String,
    external_album_id: String,
    artist_external_id: String,
    artist_name: String,
}

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(search_tracks))
        .routes(routes!(list_tracks))
        .routes(routes!(create_track))
}

#[utoipa::path(
    get,
    path = "/search",
    tag = TAG,
    params(SearchQuery),
    responses(
        (status = 200, description = "Search results across all providers", body = Vec<SearchTrackResult>),
        (status = 503, description = "Provider search unavailable"),
    )
)]
/// Search Tracks
async fn search_tracks(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Vec<SearchTrackResult>> {
    services::search::search_tracks(&state.db, &state.registry, &query)
        .await
        .map_err(app_error_response)
        .map(Json)
}

#[utoipa::path(
    get,
    path = "/",
    tag = TAG,
    responses(
        (status = 200, description = "All local library tracks", body = Vec<LibraryTrack>),
        (status = 500, description = "Failed to load tracks"),
    )
)]
/// List Tracks
async fn list_tracks(State(state): State<AppState>) -> ApiResult<Vec<LibraryTrack>> {
    services::track::list_library_tracks(&state)
        .await
        .map_err(app_error_response)
        .map(Json)
}

#[utoipa::path(
    post,
    path = "/",
    tag = TAG,
    request_body = CreateTrackRequest,
    responses(
        (status = 201, description = "Track created"),
        (status = 404, description = "Provider track or album not found"),
        (status = 500, description = "Failed to create track"),
    )
)]
/// Create Track
async fn create_track(
    State(state): State<AppState>,
    Json(request): Json<CreateTrackRequest>,
) -> ApiStatusResult {
    services::track::add_track(
        &state,
        request.provider,
        request.external_track_id,
        request.external_album_id,
        request.artist_external_id,
        request.artist_name,
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::CREATED)
}
