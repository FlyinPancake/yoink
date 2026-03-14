use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use yoink_shared::ServerAction;

use crate::{actions::dispatch_action_impl, state::AppState};

use super::helpers::{ApiErrorResponse, app_error_response, parse_uuid};

pub(crate) const TAG: &str = "Match Suggestion";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for accepting and dismissing match suggestions";

type ApiStatusResult = Result<StatusCode, ApiErrorResponse>;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(accept_match_suggestion))
        .routes(routes!(dismiss_match_suggestion))
}

/// Accept Match Suggestion
///
/// Accepts a pending match suggestion and applies the provider-link change for
/// the associated local artist or album.
#[utoipa::path(
    post,
    path = "/{suggestion_id}/accept",
    tag = TAG,
    params(
        ("suggestion_id" = String, Path, description = "Match suggestion UUID")
    ),
    responses(
        (status = 204, description = "Match suggestion accepted"),
        (status = 400, description = "Invalid suggestion id"),
        (status = 404, description = "Match suggestion not found"),
        (status = 409, description = "Suggestion conflicts with existing links"),
        (status = 500, description = "Failed to accept match suggestion"),
    )
)]
async fn accept_match_suggestion(
    State(state): State<AppState>,
    Path(suggestion_id): Path<String>,
) -> ApiStatusResult {
    let suggestion_id = parse_suggestion_id(&suggestion_id)?;
    dispatch_action_impl(state, ServerAction::AcceptMatchSuggestion { suggestion_id })
        .await
        .map_err(app_error_response)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Dismiss Match Suggestion
///
/// Marks a match suggestion as dismissed without linking the suggested provider entity.
#[utoipa::path(
    post,
    path = "/{suggestion_id}/dismiss",
    tag = TAG,
    params(
        ("suggestion_id" = String, Path, description = "Match suggestion UUID")
    ),
    responses(
        (status = 204, description = "Match suggestion dismissed"),
        (status = 400, description = "Invalid suggestion id"),
        (status = 500, description = "Failed to dismiss match suggestion"),
    )
)]
async fn dismiss_match_suggestion(
    State(state): State<AppState>,
    Path(suggestion_id): Path<String>,
) -> ApiStatusResult {
    let suggestion_id = parse_suggestion_id(&suggestion_id)?;
    dispatch_action_impl(
        state,
        ServerAction::DismissMatchSuggestion { suggestion_id },
    )
    .await
    .map_err(app_error_response)?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_suggestion_id(raw: &str) -> Result<Uuid, ApiErrorResponse> {
    parse_uuid(raw, "suggestion_id")
}
