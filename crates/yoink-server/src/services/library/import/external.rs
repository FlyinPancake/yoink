use std::path::Path;

use yoink_shared::{ImportConfirmation, ImportPreviewItem, ImportResultSummary, ManualImportMode};

use crate::{error::AppResult, state::AppState};

/// Preview an external import from a source path.
pub(crate) async fn preview_external_import(
    state: &AppState,
    source_path: String,
) -> AppResult<Vec<ImportPreviewItem>> {
    super::shared::preview_source(state, Path::new(&source_path)).await
}

/// Confirm an external import.
pub(crate) async fn confirm_external_import(
    state: &AppState,
    source_path: String,
    mode: ManualImportMode,
    items: Vec<ImportConfirmation>,
) -> AppResult<ImportResultSummary> {
    super::shared::confirm_source(state, Path::new(&source_path), Some(mode), items).await
}
