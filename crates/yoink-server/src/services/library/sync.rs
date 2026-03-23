use uuid::Uuid;

use crate::{error::AppResult, state::AppState};

/// Sync albums for an artist from all linked metadata providers.
///
/// TODO: rewrite to use SeaORM entities
pub(crate) async fn sync_artist_albums(state: &AppState, artist_id: Uuid) -> AppResult<()> {
    tracing::warn!(%artist_id, "sync_artist_albums is currently stubbed out");
    state.notify_sse();
    Ok(())
}
