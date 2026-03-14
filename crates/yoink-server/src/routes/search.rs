use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use yoink_shared::{SearchAlbumResult, SearchArtistResult, SearchTrackResult};

use crate::{server_context::build_server_context, state::AppState};

use super::helpers::{ApiErrorResponse, yoink_error_response};

pub(crate) const TAG: &str = "Search";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for aggregated provider search";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;

#[derive(Debug, Deserialize, ToSchema)]
struct SearchQuery {
    query: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
struct SearchAllResult {
    artists: Vec<SearchArtistResult>,
    albums: Vec<SearchAlbumResult>,
    tracks: Vec<SearchTrackResult>,
}

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(search_all))
}

/// Search All
///
/// Searches artists, albums, and tracks across all registered providers with a single query.
#[utoipa::path(
    get,
    path = "/",
    tag = TAG,
    params(
        ("query" = String, Query, description = "Search query")
    ),
    responses(
        (status = 200, description = "Aggregated search results", body = SearchAllResult),
        (status = 503, description = "One or more provider searches failed"),
    )
)]
async fn search_all(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> ApiResult<SearchAllResult> {
    let trimmed = query.query.trim();
    if trimmed.is_empty() {
        return Ok(Json(SearchAllResult {
            artists: Vec::new(),
            albums: Vec::new(),
            tracks: Vec::new(),
        }));
    }

    let ctx = build_server_context(&state);
    let artists = (ctx.search_artists)(trimmed.to_string())
        .await
        .map_err(yoink_error_response)?;
    let albums = (ctx.search_albums)(trimmed.to_string())
        .await
        .map_err(yoink_error_response)?;
    let tracks = (ctx.search_tracks)(trimmed.to_string())
        .await
        .map_err(yoink_error_response)?;

    Ok(Json(SearchAllResult {
        artists,
        albums,
        tracks,
    }))
}
