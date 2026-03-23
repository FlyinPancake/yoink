use axum::{
    Json,
    extract::{Query, State},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    routes::helpers::app_error_response,
    services::{
        self,
        search::{SearchAllResult, SearchQuery},
    },
    state::AppState,
};

use super::helpers::ApiErrorResponse;

pub(crate) const TAG: &str = "Search";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for aggregated provider search";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;

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
        ("query" = SearchQuery, Query, description = "Search query")
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
    let results = services::search::search_all(&state.db, &state.registry, &query)
        .await
        .map_err(app_error_response)?;
    Ok(Json(results))
}
