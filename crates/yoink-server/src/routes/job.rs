use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use yoink_shared::{DownloadJob, ServerAction};

use crate::{actions::dispatch_action_impl, state::AppState};

use super::helpers::{ApiErrorResponse, app_error_response, parse_uuid};

pub(crate) const TAG: &str = "Job";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for download job inspection and control";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;
type ApiStatusResult = Result<StatusCode, ApiErrorResponse>;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_jobs))
        .routes(routes!(cancel_job))
        .routes(routes!(clear_completed_jobs))
}

/// List Jobs
///
/// Returns the current in-memory download job list in its UI-facing sort order.
#[utoipa::path(
    get,
    path = "/",
    tag = TAG,
    responses(
        (status = 200, description = "All download jobs", body = Vec<DownloadJob>),
    )
)]
async fn list_jobs(State(state): State<AppState>) -> ApiResult<Vec<DownloadJob>> {
    Ok(Json(state.download_jobs.read().await.clone()))
}

/// Cancel Job
///
/// Marks a queued download job as cancelled and failed.
#[utoipa::path(
    post,
    path = "/{job_id}/cancel",
    tag = TAG,
    params(
        ("job_id" = String, Path, description = "Download job UUID")
    ),
    responses(
        (status = 204, description = "Job cancelled"),
        (status = 400, description = "Invalid job id"),
        (status = 500, description = "Failed to cancel job"),
    )
)]
async fn cancel_job(State(state): State<AppState>, Path(job_id): Path<String>) -> ApiStatusResult {
    let job_id = parse_job_id(&job_id)?;
    dispatch_action_impl(state, ServerAction::CancelDownload { job_id })
        .await
        .map_err(app_error_response)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Clear Completed Jobs
///
/// Deletes completed download job rows and removes them from the in-memory job list.
#[utoipa::path(
    delete,
    path = "/completed",
    tag = TAG,
    responses(
        (status = 204, description = "Completed jobs cleared"),
        (status = 500, description = "Failed to clear completed jobs"),
    )
)]
async fn clear_completed_jobs(State(state): State<AppState>) -> ApiStatusResult {
    dispatch_action_impl(state, ServerAction::ClearCompleted)
        .await
        .map_err(app_error_response)?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_job_id(raw: &str) -> Result<Uuid, ApiErrorResponse> {
    parse_uuid(raw, "job_id")
}
