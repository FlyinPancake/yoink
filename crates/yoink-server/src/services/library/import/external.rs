use yoink_shared::{ImportPreviewItem, ImportResultSummary};

use crate::{error::AppResult, state::AppState};

/// Preview an external import from a source path.
///
/// TODO: rewrite to use SeaORM entities
pub(crate) async fn preview_external_import(
    state: &AppState,
    source_path: String,
) -> AppResult<Vec<ImportPreviewItem>> {
    tracing::warn!(%source_path, "preview_external_import is currently stubbed out");
    let _ = state;
    Ok(vec![])
}

/// Confirm an external import.
///
/// TODO: rewrite to use SeaORM entities
pub(crate) async fn confirm_external_import(
    state: &AppState,
    source_path: String,
    mode: String,
    items: Vec<yoink_shared::ImportConfirmation>,
) -> AppResult<ImportResultSummary> {
    tracing::warn!(%source_path, %mode, "confirm_external_import is currently stubbed out");
    let _ = (state, items);
    Ok(ImportResultSummary {
        total_selected: 0,
        imported: 0,
        artists_added: 0,
        failed: 0,
        errors: vec![],
    })
}
