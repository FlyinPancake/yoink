use uuid::Uuid;

use crate::{
    error::AppResult,
    state::AppState,
};

/// Recompute match suggestions for an artist.
///
/// TODO: rewrite to use SeaORM entities
pub(crate) async fn recompute_artist_match_suggestions(
    state: &AppState,
    artist_id: Uuid,
) -> AppResult<()> {
    tracing::warn!(%artist_id, "recompute_artist_match_suggestions is currently stubbed out");
    let _ = state;
    Ok(())
}
