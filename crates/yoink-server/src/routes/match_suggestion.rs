use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::state::AppState;

use super::helpers::{ApiErrorResponse, app_error_response};

pub(crate) const TAG: &str = "Match Suggestion";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for accepting and dismissing match suggestions";

type ApiStatusResult = Result<StatusCode, ApiErrorResponse>;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(accept_match_suggestion))
        .routes(routes!(dismiss_match_suggestion))
}

#[utoipa::path(
    post,
    path = "/{suggestion_id}/accept",
    tag = TAG,
    params(
        ("suggestion_id" = Uuid, Path, description = "Match suggestion UUID")
    ),
    responses(
        (status = 204, description = "Match suggestion accepted"),
        (status = 404, description = "Match suggestion not found"),
        (status = 500, description = "Failed to accept match suggestion"),
    )
)]
/// Accept Match Suggestion
async fn accept_match_suggestion(
    State(state): State<AppState>,
    Path(suggestion_id): Path<Uuid>,
) -> ApiStatusResult {
    crate::actions::matching::accept_match_suggestion(&state, suggestion_id)
        .await
        .map_err(app_error_response)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/{suggestion_id}/dismiss",
    tag = TAG,
    params(
        ("suggestion_id" = Uuid, Path, description = "Match suggestion UUID")
    ),
    responses(
        (status = 204, description = "Match suggestion dismissed"),
        (status = 500, description = "Failed to dismiss match suggestion"),
    )
)]
/// Dismiss Match Suggestion
async fn dismiss_match_suggestion(
    State(state): State<AppState>,
    Path(suggestion_id): Path<Uuid>,
) -> ApiStatusResult {
    crate::actions::matching::dismiss_match_suggestion(&state, suggestion_id)
        .await
        .map_err(app_error_response)?;
    Ok(StatusCode::NO_CONTENT)
}
