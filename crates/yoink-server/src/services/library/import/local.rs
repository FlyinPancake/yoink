use crate::api::{ImportConfirmation, ImportPreviewItem, ImportResultSummary};

use crate::{error::AppResult, state::AppState};

/// Scan and import the local library.
pub(crate) async fn scan_and_import_library(state: &AppState) -> AppResult<ImportResultSummary> {
    let items = super::shared::preview_source(state, &state.music_root).await?;
    Ok(ImportResultSummary {
        total_selected: items.len(),
        imported: 0,
        artists_added: items
            .iter()
            .filter(|item| !item.already_imported && item.candidates.is_empty())
            .count(),
        failed: 0,
        errors: vec![],
    })
}

/// Preview items that would be imported from the local library.
pub(crate) async fn preview_import_library(state: &AppState) -> AppResult<Vec<ImportPreviewItem>> {
    super::shared::preview_source(state, &state.music_root).await
}

/// Confirm selected import items from the local library.
pub(crate) async fn confirm_import_library(
    state: &AppState,
    items: Vec<ImportConfirmation>,
) -> AppResult<ImportResultSummary> {
    super::shared::confirm_source(state, &state.music_root, None, items).await
}
