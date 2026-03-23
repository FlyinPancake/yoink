use yoink_shared::{ImportConfirmation, ImportPreviewItem, ImportResultSummary};

use crate::{error::AppResult, state::AppState};

/// Scan and import the local library.
///
/// TODO: rewrite to use SeaORM entities
pub(crate) async fn scan_and_import_library(state: &AppState) -> AppResult<ImportResultSummary> {
    tracing::warn!("scan_and_import_library is currently stubbed out");
    let _ = state;
    Ok(ImportResultSummary {
        total_selected: 0,
        imported: 0,
        artists_added: 0,
        failed: 0,
        errors: vec![],
    })
}

/// Preview items that would be imported from the local library.
///
/// TODO: rewrite to use SeaORM entities
pub(crate) async fn preview_import_library(
    state: &AppState,
) -> AppResult<Vec<ImportPreviewItem>> {
    tracing::warn!("preview_import_library is currently stubbed out");
    let _ = state;
    Ok(vec![])
}

/// Confirm selected import items from the local library.
///
/// TODO: rewrite to use SeaORM entities
pub(crate) async fn confirm_import_library(
    state: &AppState,
    items: Vec<ImportConfirmation>,
) -> AppResult<ImportResultSummary> {
    tracing::warn!("confirm_import_library is currently stubbed out");
    let _ = (state, items);
    Ok(ImportResultSummary {
        total_selected: 0,
        imported: 0,
        artists_added: 0,
        failed: 0,
        errors: vec![],
    })
}
