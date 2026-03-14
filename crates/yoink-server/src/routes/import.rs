use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use yoink_shared::{
    BrowseEntry, ExternalImportConfirmation, ImportConfirmation, ImportPreviewItem,
    ImportResultSummary,
};

use crate::{actions::dispatch_action_impl, server_context::build_server_context, state::AppState};

use super::helpers::{ApiErrorResponse, app_error_response, yoink_error_response};

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

/// Preview Import
///
/// Scans the configured music library for import candidates and returns the
/// current preview set.
#[utoipa::path(
    get,
    path = "/preview",
    tag = TAG,
    responses(
        (status = 200, description = "Import preview items", body = Vec<ImportPreviewItem>),
        (status = 500, description = "Failed to preview import"),
    )
)]
async fn preview_import(State(state): State<AppState>) -> ApiResult<Vec<ImportPreviewItem>> {
    let ctx = build_server_context(&state);
    let items = (ctx.preview_import)().await.map_err(yoink_error_response)?;
    Ok(Json(items))
}

/// Scan Import
///
/// Triggers a scan-and-import run using the existing server-side import action.
#[utoipa::path(
    post,
    path = "/scan",
    tag = TAG,
    responses(
        (status = 204, description = "Import scan started"),
        (status = 500, description = "Failed to start import scan"),
    )
)]
async fn scan_import(State(state): State<AppState>) -> ApiStatusResult {
    dispatch_action_impl(state, yoink_shared::ServerAction::ScanImportLibrary)
        .await
        .map_err(app_error_response)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Confirm Import
///
/// Confirms a library-scan import selection and imports the chosen albums.
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
async fn confirm_import(
    State(state): State<AppState>,
    Json(items): Json<Vec<ImportConfirmation>>,
) -> ApiResult<ImportResultSummary> {
    let ctx = build_server_context(&state);
    let summary = (ctx.confirm_import)(items)
        .await
        .map_err(yoink_error_response)?;
    Ok(Json(summary))
}

/// Browse Path
///
/// Lists directories and recognised audio files for a server-side filesystem path.
#[utoipa::path(
    post,
    path = "/browse",
    tag = TAG,
    request_body = BrowsePathRequest,
    responses(
        (status = 200, description = "Filesystem browse entries", body = Vec<BrowseEntry>),
        (status = 400, description = "Invalid path"),
        (status = 500, description = "Failed to browse path"),
    )
)]
async fn browse_path(
    State(state): State<AppState>,
    Json(request): Json<BrowsePathRequest>,
) -> ApiResult<Vec<BrowseEntry>> {
    let ctx = build_server_context(&state);
    let entries = (ctx.browse_path)(request.path)
        .await
        .map_err(yoink_error_response)?;
    Ok(Json(entries))
}

/// Preview External Import
///
/// Scans an arbitrary server-side source path and returns external import candidates.
#[utoipa::path(
    post,
    path = "/external/preview",
    tag = TAG,
    request_body = PreviewExternalImportRequest,
    responses(
        (status = 200, description = "External import preview items", body = Vec<ImportPreviewItem>),
        (status = 400, description = "Invalid source path"),
        (status = 500, description = "Failed to preview external import"),
    )
)]
async fn preview_external_import(
    State(state): State<AppState>,
    Json(request): Json<PreviewExternalImportRequest>,
) -> ApiResult<Vec<ImportPreviewItem>> {
    let ctx = build_server_context(&state);
    let items = (ctx.preview_external_import)(request.source_path)
        .await
        .map_err(yoink_error_response)?;
    Ok(Json(items))
}

/// Confirm External Import
///
/// Confirms an external import selection and imports the chosen files via copy
/// or hardlink.
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
async fn confirm_external_import(
    State(state): State<AppState>,
    Json(confirmation): Json<ExternalImportConfirmation>,
) -> ApiResult<ImportResultSummary> {
    let ctx = build_server_context(&state);
    let summary = (ctx.confirm_external_import)(confirmation)
        .await
        .map_err(yoink_error_response)?;
    Ok(Json(summary))
}
