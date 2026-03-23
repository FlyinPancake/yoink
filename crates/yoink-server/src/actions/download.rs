use uuid::Uuid;

use crate::{error::AppResult, state::AppState};

/// TODO: rewrite to use SeaORM entities
pub(crate) async fn cancel_download(state: &AppState, job_id: Uuid) -> AppResult<()> {
    tracing::warn!(%job_id, "cancel_download is currently stubbed out");
    state.notify_sse();
    Ok(())
}

/// TODO: rewrite to use SeaORM entities
pub(crate) async fn clear_completed(state: &AppState) -> AppResult<()> {
    tracing::warn!("clear_completed is currently stubbed out");
    state.notify_sse();
    Ok(())
}

/// TODO: rewrite to use SeaORM entities
pub(crate) async fn retry_download(state: &AppState, album_id: Uuid) -> AppResult<()> {
    tracing::warn!(%album_id, "retry_download is currently stubbed out");
    state.notify_sse();
    Ok(())
}
