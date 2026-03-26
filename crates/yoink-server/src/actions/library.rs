use tracing::info;

use yoink_shared::{ImportConfirmation, ManualImportMode};

use crate::{error::AppResult, services, state::AppState};

pub(crate) async fn retag_library(state: &AppState) -> AppResult<()> {
    let s = state.clone();
    tokio::spawn(async move {
        match services::retag_existing_files(&s).await {
            Ok((tagged, missing, albums)) => {
                info!(
                    tagged_files = tagged,
                    missing_files = missing,
                    scanned_albums = albums,
                    "Completed manual library retag"
                );
            }
            Err(err) => {
                info!(error = %err, "Library retag failed");
            }
        }
    });
    Ok(())
}

pub(crate) async fn scan_import_library(state: &AppState) -> AppResult<()> {
    let s = state.clone();
    tokio::spawn(async move {
        match services::scan_and_import_library(&s).await {
            Ok(summary) => {
                info!(
                    imported = summary.imported,
                    failed = summary.failed,
                    "Completed scan/import pass"
                );
            }
            Err(err) => {
                info!(error = %err, "Scan/import failed");
            }
        }
    });
    Ok(())
}

pub(crate) async fn confirm_import(
    state: &AppState,
    items: Vec<ImportConfirmation>,
) -> AppResult<()> {
    let summary = services::confirm_import_library(state, items).await?;
    info!(
        imported = summary.imported,
        failed = summary.failed,
        "Confirmed import completed"
    );
    Ok(())
}

pub(crate) async fn confirm_external_import(
    state: &AppState,
    source_path: String,
    mode: ManualImportMode,
    items: Vec<ImportConfirmation>,
) -> AppResult<()> {
    let summary = services::confirm_external_import(state, source_path, mode, items).await?;
    info!(
        imported = summary.imported,
        failed = summary.failed,
        "External import completed"
    );
    Ok(())
}
