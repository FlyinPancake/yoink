use tracing::info;

use crate::{error::AppResult, services, state::AppState};

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
