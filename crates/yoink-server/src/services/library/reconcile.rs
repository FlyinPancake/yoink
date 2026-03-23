use crate::{error::AppResult, state::AppState};

/// Reconcile library files on disk with the database.
///
/// TODO: rewrite to use SeaORM entities
pub(crate) async fn reconcile_library_files(state: &AppState) -> AppResult<usize> {
    tracing::warn!("reconcile_library_files is currently stubbed out");
    let _ = state;
    Ok(0)
}
