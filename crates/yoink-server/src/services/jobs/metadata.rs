use std::time::Duration;

use crate::{db, state::AppState};

/// Fetch album cover art JPEG bytes for embedding into track tags.
///
/// Tries the metadata provider's `fetch_cover_art_bytes` first (using a cover
/// reference from `album_extra`), then falls back to downloading the album's
/// stored `cover_url` directly.
pub async fn fetch_album_cover_art(
    state: &AppState,
    album: &db::album::ModelEx,
) -> Option<Vec<u8>> {
    if let Some(ref cover_url) = album.cover_url {
        // TODO remove hardcoded path + support non-local cover_urls
        let cover_url = format!("http://localhost:3000{}", cover_url);
        match state
            .http
            .get(cover_url.to_string())
            .timeout(Duration::from_secs(20))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(bytes) = resp.bytes().await {
                    tracing::debug!(url = %cover_url, bytes = bytes.len(), "Fetched cover art from cover_url");
                    return Some(bytes.to_vec());
                }
            }
            Ok(resp) => {
                tracing::warn!(url = %cover_url, status = %resp.status(), "cover_url returned non-success status");
            }
            Err(err) => {
                tracing::warn!(url = %cover_url, error = %err, "Failed to fetch cover art from cover_url");
            }
        }
    }

    tracing::debug!(album_id = %album.id, "No cover art available for album");
    None
}
