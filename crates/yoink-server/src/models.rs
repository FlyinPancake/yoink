use serde::Deserialize;

// Re-export shared types so the rest of the binary crate can keep using
// `crate::models::MonitoredAlbum` etc. without changes.
pub(crate) use yoink_shared::{
    DownloadJob, DownloadStatus, MonitoredAlbum, MonitoredArtist, SearchAlbumResult,
    SearchArtistResult, SearchTrackResult, TrackInfo,
};

// ── Server-only types (not needed in WASM client) ───────────

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SearchQuery {
    pub(crate) q: Option<String>,
}
