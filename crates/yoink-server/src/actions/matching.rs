use uuid::Uuid;

use crate::{
    error::AppResult,
    state::AppState,
};

/// TODO: rewrite to use SeaORM entities
pub(crate) async fn accept_match_suggestion(
    state: &AppState,
    suggestion_id: Uuid,
) -> AppResult<()> {
    tracing::warn!(%suggestion_id, "accept_match_suggestion is currently stubbed out");
    state.notify_sse();
    Ok(())
}

/// TODO: rewrite to use SeaORM entities
pub(crate) async fn dismiss_match_suggestion(
    state: &AppState,
    suggestion_id: Uuid,
) -> AppResult<()> {
    tracing::warn!(%suggestion_id, "dismiss_match_suggestion is currently stubbed out");
    state.notify_sse();
    Ok(())
}

/// TODO: rewrite to use SeaORM entities
pub(crate) async fn refresh_match_suggestions(
    state: &AppState,
    artist_id: Uuid,
) -> AppResult<()> {
    crate::services::recompute_artist_match_suggestions(state, artist_id).await?;
    state.notify_sse();
    Ok(())
}
