use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ImportConfirmation, ManualImportMode, Quality};

/// A user-initiated action that the server can execute.
///
/// Serialized by the WASM client, sent to the `dispatch_action` server function,
/// and executed on the server where `AppState` is available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerAction {
    ToggleAlbumMonitor {
        album_id: Uuid,
        monitored: bool,
    },
    SetAlbumQuality {
        album_id: Uuid,
        quality: Option<Quality>,
    },
    BulkMonitor {
        artist_id: Uuid,
        monitored: bool,
    },
    SyncArtistAlbums {
        artist_id: Uuid,
    },
    RemoveArtist {
        artist_id: Uuid,
        remove_files: bool,
    },
    AddArtist {
        name: String,
        provider: String,
        external_id: String,
        image_url: Option<String>,
        external_url: Option<String>,
    },
    LinkArtistProvider {
        artist_id: Uuid,
        provider: String,
        external_id: String,
        external_url: Option<String>,
        external_name: Option<String>,
        image_ref: Option<String>,
    },
    UnlinkArtistProvider {
        artist_id: Uuid,
        provider: String,
        external_id: String,
    },
    CancelDownload {
        job_id: Uuid,
    },
    ClearCompleted,
    RetryDownload {
        album_id: Uuid,
    },
    RemoveAlbumFiles {
        album_id: Uuid,
        unmonitor: bool,
    },
    AcceptMatchSuggestion {
        suggestion_id: Uuid,
    },
    DismissMatchSuggestion {
        suggestion_id: Uuid,
    },
    RefreshMatchSuggestions {
        artist_id: Uuid,
    },
    MergeAlbums {
        target_album_id: Uuid,
        source_album_id: Uuid,
        /// If provided, override the surviving album's title.
        result_title: Option<String>,
        /// If provided, override the surviving album's cover URL.
        result_cover_url: Option<String>,
    },
    AddAlbumArtist {
        album_id: Uuid,
        artist_id: Uuid,
    },
    RemoveAlbumArtist {
        album_id: Uuid,
        artist_id: Uuid,
    },
    RetagLibrary,
    ScanImportLibrary,
    ConfirmImport {
        items: Vec<ImportConfirmation>,
    },
    UpdateArtist {
        artist_id: Uuid,
        name: Option<String>,
        image_url: Option<String>,
    },
    FetchArtistBio {
        artist_id: Uuid,
    },
    /// Toggle whether an artist is fully monitored (discography synced).
    /// When promoted to monitored, triggers a full discography sync.
    ToggleArtistMonitor {
        artist_id: Uuid,
        monitored: bool,
    },
    /// Toggle monitoring for an individual track.
    /// When monitored, the track will be downloaded independently.
    ToggleTrackMonitor {
        track_id: Uuid,
        album_id: Uuid,
        monitored: bool,
    },
    SetTrackQuality {
        album_id: Uuid,
        track_id: Uuid,
        quality: Option<Quality>,
    },
    /// Add an album directly from search results.
    /// Creates a lightweight (unmonitored) artist if one doesn't exist,
    /// fetches full album metadata + tracks from the provider, and stores them.
    AddAlbum {
        provider: String,
        external_album_id: String,
        /// Provider-specific external artist ID.
        artist_external_id: String,
        artist_name: String,
        /// If true, monitor all tracks on the album for download.
        monitor_all: bool,
    },
    /// Add a single track from search results.
    /// Creates the parent album + lightweight artist as needed,
    /// and marks just this track as monitored.
    AddTrack {
        provider: String,
        external_track_id: String,
        external_album_id: String,
        artist_external_id: String,
        artist_name: String,
    },
    /// Set monitoring for all tracks on an album at once.
    BulkToggleTrackMonitor {
        album_id: Uuid,
        monitored: bool,
    },
    /// Import files from an external path into the music library via copy or
    /// hardlink.
    ConfirmExternalImport {
        source_path: String,
        mode: ManualImportMode,
        items: Vec<ImportConfirmation>,
    },
}
