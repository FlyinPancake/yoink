//! Server-side context shared between API route handlers.

use uuid::Uuid;

use crate::{
    ArtistImageOption, BrowseEntry, DownloadJob, ExternalImportConfirmation, ImportConfirmation,
    ImportPreviewItem, ImportResultSummary, LibraryTrack, MatchSuggestion, MonitoredAlbum,
    MonitoredArtist, ProviderLink, Quality, SearchAlbumResult, SearchArtistResult,
    SearchTrackResult, ServerAction, TrackInfo, YoinkError,
};

type AsyncFnResult<T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, YoinkError>> + Send>>;

pub type SearchArtistsFn =
    std::sync::Arc<dyn Fn(String) -> AsyncFnResult<Vec<SearchArtistResult>> + Send + Sync>;

pub type SearchArtistsScopedFn =
    std::sync::Arc<dyn Fn(String, String) -> AsyncFnResult<Vec<SearchArtistResult>> + Send + Sync>;

pub type ListProvidersFn = std::sync::Arc<dyn Fn() -> Vec<String> + Send + Sync>;

pub type FetchTracksFn =
    std::sync::Arc<dyn Fn(Uuid) -> AsyncFnResult<Vec<TrackInfo>> + Send + Sync>;

pub type FetchArtistLinksFn =
    std::sync::Arc<dyn Fn(Uuid) -> AsyncFnResult<Vec<ProviderLink>> + Send + Sync>;

pub type FetchAlbumLinksFn =
    std::sync::Arc<dyn Fn(Uuid) -> AsyncFnResult<Vec<ProviderLink>> + Send + Sync>;

pub type FetchArtistMatchSuggestionsFn =
    std::sync::Arc<dyn Fn(Uuid) -> AsyncFnResult<Vec<MatchSuggestion>> + Send + Sync>;

pub type FetchAlbumMatchSuggestionsFn =
    std::sync::Arc<dyn Fn(Uuid) -> AsyncFnResult<Vec<MatchSuggestion>> + Send + Sync>;

pub type DispatchActionFn = std::sync::Arc<dyn Fn(ServerAction) -> AsyncFnResult<()> + Send + Sync>;

pub type PreviewImportFn =
    std::sync::Arc<dyn Fn() -> AsyncFnResult<Vec<ImportPreviewItem>> + Send + Sync>;

pub type ConfirmImportFn = std::sync::Arc<
    dyn Fn(Vec<ImportConfirmation>) -> AsyncFnResult<ImportResultSummary> + Send + Sync,
>;

pub type FetchArtistImagesFn =
    std::sync::Arc<dyn Fn(Uuid) -> AsyncFnResult<Vec<ArtistImageOption>> + Send + Sync>;

pub type SearchAlbumsFn =
    std::sync::Arc<dyn Fn(String) -> AsyncFnResult<Vec<SearchAlbumResult>> + Send + Sync>;

pub type SearchTracksFn =
    std::sync::Arc<dyn Fn(String) -> AsyncFnResult<Vec<SearchTrackResult>> + Send + Sync>;

pub type FetchLibraryTracksFn =
    std::sync::Arc<dyn Fn() -> AsyncFnResult<Vec<LibraryTrack>> + Send + Sync>;

pub type BrowsePathFn =
    std::sync::Arc<dyn Fn(String) -> AsyncFnResult<Vec<BrowseEntry>> + Send + Sync>;

pub type PreviewExternalImportFn =
    std::sync::Arc<dyn Fn(String) -> AsyncFnResult<Vec<ImportPreviewItem>> + Send + Sync>;

pub type ConfirmExternalImportFn = std::sync::Arc<
    dyn Fn(ExternalImportConfirmation) -> AsyncFnResult<ImportResultSummary> + Send + Sync,
>;

/// Holds the shared in-memory state that API route handlers need to read.
///
/// Built from `AppState` and passed to route handlers that need access
/// to provider-backed operations.
#[derive(Clone)]
pub struct ServerContext {
    pub auth_enabled: bool,
    pub monitored_artists: std::sync::Arc<tokio::sync::RwLock<Vec<MonitoredArtist>>>,
    pub monitored_albums: std::sync::Arc<tokio::sync::RwLock<Vec<MonitoredAlbum>>>,
    pub download_jobs: std::sync::Arc<tokio::sync::RwLock<Vec<DownloadJob>>>,
    pub default_quality: Quality,
    pub search_artists: SearchArtistsFn,
    pub search_artists_scoped: SearchArtistsScopedFn,
    pub list_providers: ListProvidersFn,
    pub fetch_tracks: FetchTracksFn,
    pub fetch_artist_links: FetchArtistLinksFn,
    pub fetch_album_links: FetchAlbumLinksFn,
    pub fetch_artist_match_suggestions: FetchArtistMatchSuggestionsFn,
    pub fetch_album_match_suggestions: FetchAlbumMatchSuggestionsFn,
    pub dispatch_action: DispatchActionFn,
    pub preview_import: PreviewImportFn,
    pub confirm_import: ConfirmImportFn,
    pub fetch_artist_images: FetchArtistImagesFn,
    pub search_albums: SearchAlbumsFn,
    pub search_tracks: SearchTracksFn,
    pub fetch_library_tracks: FetchLibraryTracksFn,
    pub browse_path: BrowsePathFn,
    pub preview_external_import: PreviewExternalImportFn,
    pub confirm_external_import: ConfirmExternalImportFn,
}
