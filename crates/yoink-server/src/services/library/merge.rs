use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    state::AppState,
};

/// Merge a source album into a target album.
///
/// TODO: rewrite to use SeaORM entities
pub(crate) async fn merge_albums(
    state: &AppState,
    target_album_id: Uuid,
    source_album_id: Uuid,
    _result_title: Option<&str>,
    _result_cover_url: Option<&str>,
) -> AppResult<()> {
    if target_album_id == source_album_id {
        return Err(AppError::validation(
            Some("source_album_id"),
            "target and source album must be different",
        ));
    }

    tracing::warn!(%target_album_id, %source_album_id, "merge_albums is currently stubbed out");
    state.notify_sse();
    Ok(())
}
