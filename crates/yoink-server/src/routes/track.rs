use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use yoink_shared::{LibraryTrack, SearchTrackResult, ServerAction};

use crate::{actions::dispatch_action_impl, server_context::build_server_context, state::AppState};

use super::helpers::{
    ApiErrorResponse, app_error_response, enrich_track_results, yoink_error_response,
};

pub(crate) const TAG: &str = "Track";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for track search and library track access";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;
type ApiStatusResult = Result<StatusCode, ApiErrorResponse>;

#[derive(Debug, Deserialize, ToSchema)]
struct TrackSearchQuery {
    query: String,
}

#[derive(Debug, Deserialize, ToSchema)]
struct CreateTrackRequest {
    provider: String,
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

/// Search Tracks
///
/// Searches all registered metadata providers for tracks matching the query
/// string and returns the aggregated results.
#[utoipa::path(
    get,
    path = "/search",
    tag = TAG,
    params(
        ("query" = String, Query, description = "Track search query")
    ),
    responses(
        (status = 200, description = "Search results across all providers", body = Vec<SearchTrackResult>),
        (status = 503, description = "Provider search unavailable"),
    )
)]
async fn search_tracks(
    State(state): State<AppState>,
    Query(query): Query<TrackSearchQuery>,
) -> ApiResult<Vec<SearchTrackResult>> {
    let trimmed = query.query.trim();
    if trimmed.is_empty() {
        return Ok(Json(Vec::new()));
    }

    let ctx = build_server_context(&state);
    let mut results = (ctx.search_tracks)(trimmed.to_string())
        .await
        .map_err(yoink_error_response)?;
    enrich_track_results(&state.db, &mut results).await;
    Ok(Json(results))
}

/// List Tracks
///
/// Returns the library-wide track view, including parent album and artist
/// context for each local track row.
#[utoipa::path(
    get,
    path = "/",
    tag = TAG,
    responses(
        (status = 200, description = "All local library tracks", body = Vec<LibraryTrack>),
        (status = 500, description = "Failed to load tracks"),
    )
)]
async fn list_tracks(State(state): State<AppState>) -> ApiResult<Vec<LibraryTrack>> {
    let ctx = build_server_context(&state);
    let tracks = (ctx.fetch_library_tracks)()
        .await
        .map_err(yoink_error_response)?;
    Ok(Json(tracks))
}

/// Create Track
///
/// Adds a single provider track to the local library, creating the parent
/// lightweight artist and album as needed and marking only that track wanted.
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
async fn create_track(
    State(state): State<AppState>,
    Json(request): Json<CreateTrackRequest>,
) -> ApiStatusResult {
    dispatch_action_impl(
        state,
        ServerAction::AddTrack {
            provider: request.provider,
            external_track_id: request.external_track_id,
            external_album_id: request.external_album_id,
            artist_external_id: request.artist_external_id,
            artist_name: request.artist_name,
        },
    )
    .await
    .map_err(app_error_response)?;

    Ok(StatusCode::CREATED)
}
