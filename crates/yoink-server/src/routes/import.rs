use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    api::{
        BrowseEntry, ExternalImportConfirmation, ImportConfirmation, ImportPreviewItem,
        ImportResultSummary,
    },
    services,
    state::AppState,
};

use super::helpers::{ApiErrorResponse, app_error_response};

pub(crate) const TAG: &str = "Import";
pub(crate) const TAG_DESCRIPTION: &str = "Endpoints for local and external library import flows";

type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;
type ApiStatusResult = Result<StatusCode, ApiErrorResponse>;

#[derive(Debug, Deserialize, ToSchema)]
struct BrowsePathRequest {
    path: String,
}

#[derive(Debug, Deserialize, ToSchema)]
struct PreviewExternalImportRequest {
    source_path: String,
}

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(preview_import))
        .routes(routes!(scan_import))
        .routes(routes!(confirm_import))
        .routes(routes!(browse_path))
        .routes(routes!(preview_external_import))
        .routes(routes!(confirm_external_import))
}

#[utoipa::path(
    get,
    path = "/preview",
    tag = TAG,
    responses(
        (status = 200, description = "Import preview items", body = Vec<ImportPreviewItem>),
        (status = 500, description = "Failed to preview import"),
    )
)]
/// Preview Import
async fn preview_import(State(state): State<AppState>) -> ApiResult<Vec<ImportPreviewItem>> {
    let items = services::preview_import_library(&state)
        .await
        .map_err(app_error_response)?;
    Ok(Json(items))
}

#[utoipa::path(
    post,
    path = "/scan",
    tag = TAG,
    responses(
        (status = 204, description = "Import scan started"),
        (status = 500, description = "Failed to start import scan"),
    )
)]
/// Scan Import
async fn scan_import(State(state): State<AppState>) -> ApiStatusResult {
    crate::actions::library::scan_import_library(&state)
        .await
        .map_err(app_error_response)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/confirm",
    tag = TAG,
    request_body = Vec<ImportConfirmation>,
    responses(
        (status = 200, description = "Import result summary", body = ImportResultSummary),
        (status = 500, description = "Failed to confirm import"),
    )
)]
/// Confirm Import
async fn confirm_import(
    State(state): State<AppState>,
    Json(items): Json<Vec<ImportConfirmation>>,
) -> ApiResult<ImportResultSummary> {
    let summary = services::confirm_import_library(&state, items)
        .await
        .map_err(app_error_response)?;
    Ok(Json(summary))
}

#[utoipa::path(
    post,
    path = "/browse",
    tag = TAG,
    request_body = BrowsePathRequest,
    responses(
        (status = 200, description = "Filesystem browse entries", body = Vec<BrowseEntry>),
        (status = 500, description = "Failed to browse path"),
    )
)]
/// Browse Path
async fn browse_path(
    State(_state): State<AppState>,
    Json(request): Json<BrowsePathRequest>,
) -> ApiResult<Vec<BrowseEntry>> {
    let entries = services::browse_path(&request.path)
        .await
        .map_err(app_error_response)?;
    Ok(Json(entries))
}

#[utoipa::path(
    post,
    path = "/external/preview",
    tag = TAG,
    request_body = PreviewExternalImportRequest,
    responses(
        (status = 200, description = "External import preview items", body = Vec<ImportPreviewItem>),
        (status = 500, description = "Failed to preview external import"),
    )
)]
/// Preview External Import
async fn preview_external_import(
    State(state): State<AppState>,
    Json(request): Json<PreviewExternalImportRequest>,
) -> ApiResult<Vec<ImportPreviewItem>> {
    let items = services::preview_external_import(&state, request.source_path)
        .await
        .map_err(app_error_response)?;
    Ok(Json(items))
}

#[utoipa::path(
    post,
    path = "/external/confirm",
    tag = TAG,
    request_body = ExternalImportConfirmation,
    responses(
        (status = 200, description = "External import result summary", body = ImportResultSummary),
        (status = 500, description = "Failed to confirm external import"),
    )
)]
/// Confirm External Import
async fn confirm_external_import(
    State(state): State<AppState>,
    Json(request): Json<ExternalImportConfirmation>,
) -> ApiResult<ImportResultSummary> {
    let summary =
        services::confirm_external_import(&state, request.source_path, request.mode, request.items)
            .await
            .map_err(app_error_response)?;
    Ok(Json(summary))
}
