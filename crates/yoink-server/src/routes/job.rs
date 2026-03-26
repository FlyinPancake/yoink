use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use yoink_shared::DownloadJob;

use crate::state::AppState;

use super::helpers::{ApiErrorResponse, app_error_response};

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

#[utoipa::path(
    get,
    path = "/",
    tag = TAG,
    responses(
        (status = 200, description = "All download jobs", body = Vec<DownloadJob>),
    )
)]
/// List Jobs
async fn list_jobs(State(_state): State<AppState>) -> ApiResult<Vec<DownloadJob>> {
    crate::services::downloads::list_jobs(&_state)
        .await
        .map(Json)
        .map_err(app_error_response)
}

#[utoipa::path(
    post,
    path = "/{job_id}/cancel",
    tag = TAG,
    params(
        ("job_id" = Uuid, Path, description = "Download job UUID")
    ),
    responses(
        (status = 204, description = "Job cancelled"),
        (status = 500, description = "Failed to cancel job"),
    )
)]
/// Cancel Job
async fn cancel_job(State(state): State<AppState>, Path(job_id): Path<Uuid>) -> ApiStatusResult {
    crate::services::downloads::cancel_job(&state, job_id)
        .await
        .map_err(app_error_response)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/completed",
    tag = TAG,
    responses(
        (status = 204, description = "Completed jobs cleared"),
        (status = 500, description = "Failed to clear completed jobs"),
    )
)]
/// Clear Completed Jobs
async fn clear_completed_jobs(State(state): State<AppState>) -> ApiStatusResult {
    crate::services::downloads::clear_completed_jobs(&state)
        .await
        .map_err(app_error_response)?;
    Ok(StatusCode::NO_CONTENT)
}
