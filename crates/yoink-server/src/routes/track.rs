use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::api::{LibraryTrack, SearchTrackResult};

use crate::{
    db::provider::Provider,
    providers::{ManualDownloadSelection, ManualSearchCandidate},
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
        .routes(routes!(manual_search_track))
        .routes(routes!(manual_download_track))
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
    get,
    path = "/{track_id}/manual-search",
    tag = TAG,
    params(("track_id" = Uuid, Path, description = "Library track id")),
    responses(
        (status = 200, description = "All candidate files found for the track, best-scored first", body = Vec<ManualSearchCandidate>),
        (status = 404, description = "Track not found"),
        (status = 503, description = "No search-capable download provider available"),
    )
)]
/// Manual Search
///
/// Interactive search: returns every candidate file the download provider
/// surfaces for this track, including ones the automatic matcher would
/// reject, so the user can pick one manually. Can take up to a minute.
async fn manual_search_track(
    State(state): State<AppState>,
    Path(track_id): Path<Uuid>,
) -> ApiResult<Vec<ManualSearchCandidate>> {
    services::jobs::download::manual_search_track(&state, track_id)
        .await
        .map_err(app_error_response)
        .map(Json)
}

#[utoipa::path(
    post,
    path = "/{track_id}/manual-download",
    tag = TAG,
    params(("track_id" = Uuid, Path, description = "Library track id")),
    request_body = ManualDownloadSelection,
    responses(
        (status = 202, description = "Manual download job enqueued"),
        (status = 404, description = "Track not found"),
        (status = 500, description = "Failed to enqueue download"),
    )
)]
/// Manual Download
///
/// Enqueue a download of a specific user-chosen file for this track,
/// bypassing automatic candidate selection.
async fn manual_download_track(
    State(state): State<AppState>,
    Path(track_id): Path<Uuid>,
    Json(selection): Json<ManualDownloadSelection>,
) -> ApiStatusResult {
    services::jobs::download::enqueue_manual_download_job(&state, track_id, selection)
        .await
        .map_err(app_error_response)?;

    Ok(StatusCode::ACCEPTED)
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
