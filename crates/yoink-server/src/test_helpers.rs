//! Shared test infrastructure for integration tests.
//!
//! Provides helpers for creating in-memory databases, mock providers,
//! pre-populated `AppState` instances, and seed data builders.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use tokio::sync::{Mutex, Notify, RwLock, broadcast};
use uuid::Uuid;

use crate::db;
use crate::models::{DownloadJob, DownloadStatus, MonitoredAlbum, MonitoredArtist, TrackInfo};
use crate::providers::registry::ProviderRegistry;
use crate::providers::{
    DownloadSource, DownloadTrackContext, MetadataProvider, PlaybackInfo, ProviderAlbum,
    ProviderArtist, ProviderError, ProviderSearchAlbum, ProviderSearchTrack, ProviderTrack,
};
use crate::state::AppState;
use crate::{app_config::AuthConfig, auth::AuthService};
use yoink_shared::Quality;

// ── Database helpers ────────────────────────────────────────────────

/// Create an in-memory SQLite pool with all migrations applied.
pub(crate) async fn test_db() -> SqlitePool {
    db::open("sqlite::memory:")
        .await
        .expect("failed to open in-memory test db")
}

// ── AppState helpers ────────────────────────────────────────────────

/// Build a minimal `AppState` suitable for integration tests.
///
/// Uses an in-memory DB (empty), a tempdir for `music_root`, and an empty
/// provider registry. The `download_notify` and `sse_tx` are real but unused.
pub(crate) async fn test_app_state() -> (AppState, tempfile::TempDir) {
    test_app_state_with_registry(ProviderRegistry::new()).await
}

pub(crate) async fn test_app_state_with_auth() -> (AppState, tempfile::TempDir) {
    test_app_state_with_registry_and_auth(
        ProviderRegistry::new(),
        AuthConfig {
            enabled: true,
            session_secret: "test-session-secret".to_string(),
            init_admin_username: Some("admin".to_string()),
            init_admin_password: Some("password123".to_string()),
        },
    )
    .await
}

/// Build an `AppState` with a custom `ProviderRegistry`.
pub(crate) async fn test_app_state_with_registry(
    registry: ProviderRegistry,
) -> (AppState, tempfile::TempDir) {
    test_app_state_with_registry_and_auth(
        registry,
        AuthConfig {
            enabled: false,
            session_secret: String::new(),
            init_admin_username: None,
            init_admin_password: None,
        },
    )
    .await
}

pub(crate) async fn test_app_state_with_registry_and_auth(
    registry: ProviderRegistry,
    auth_config: AuthConfig,
) -> (AppState, tempfile::TempDir) {
    let pool = test_db().await;
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let (sse_tx, _) = broadcast::channel(16);
    let auth = AuthService::new(auth_config, pool.clone())
        .await
        .expect("failed to create test auth service");

    let state = AppState {
        http: reqwest::Client::new(),
        db: pool,
        monitored_artists: Arc::new(RwLock::new(Vec::new())),
        monitored_albums: Arc::new(RwLock::new(Vec::new())),
        download_jobs: Arc::new(RwLock::new(Vec::new())),
        download_notify: Arc::new(Notify::new()),
        sse_tx,
        music_root: tmp.path().to_path_buf(),
        default_quality: Quality::High,
        download_lyrics: false,
        download_max_parallel_tracks: 2,
        registry: Arc::new(registry),
        auth: Arc::new(auth),
    };
    (state, tmp)
}

// ── Seed helpers ────────────────────────────────────────────────────

/// Insert a monitored artist into the DB and return it.
pub(crate) async fn seed_artist(pool: &SqlitePool, name: &str) -> MonitoredArtist {
    let artist = MonitoredArtist {
        id: Uuid::now_v7(),
        name: name.to_string(),
        image_url: None,
        bio: None,
        monitored: true,
        added_at: Utc::now(),
    };
    db::upsert_artist(pool, &artist)
        .await
        .expect("seed_artist failed");
    artist
}

/// Insert a monitored album into the DB and return it.
pub(crate) async fn seed_album(pool: &SqlitePool, artist_id: Uuid, title: &str) -> MonitoredAlbum {
    let album = MonitoredAlbum {
        id: Uuid::now_v7(),
        artist_id,
        artist_ids: vec![artist_id],
        artist_credits: Vec::new(),
        title: title.to_string(),
        album_type: Some("album".to_string()),
        release_date: Some("2024-01-15".to_string()),
        cover_url: None,
        explicit: false,
        quality_override: None,
        monitored: true,
        acquired: false,
        wanted: true,
        partially_wanted: false,
        added_at: Utc::now(),
    };
    db::upsert_album(pool, &album)
        .await
        .expect("seed_album failed");
    album
}

/// Insert N tracks for an album and return them.
pub(crate) async fn seed_tracks(pool: &SqlitePool, album_id: Uuid, count: u32) -> Vec<TrackInfo> {
    let mut tracks = Vec::with_capacity(count as usize);
    for i in 1..=count {
        let track = TrackInfo {
            id: Uuid::now_v7(),
            title: format!("Track {i}"),
            version: None,
            disc_number: 1,
            track_number: i,
            duration_secs: 180 + i * 10,
            duration_display: format!("{}:{:02}", (180 + i * 10) / 60, (180 + i * 10) % 60),
            isrc: Some(format!("USRC1234{i:04}")),
            explicit: false,
            quality_override: None,
            track_artist: None,
            file_path: None,
            monitored: false,
            acquired: false,
        };
        db::upsert_track(pool, &track, album_id)
            .await
            .expect("seed_tracks failed");
        tracks.push(track);
    }
    tracks
}

/// Insert an artist provider link and return the link id.
pub(crate) async fn seed_artist_provider_link(
    pool: &SqlitePool,
    artist_id: Uuid,
    provider: &str,
    external_id: &str,
) -> Uuid {
    let link = db::ArtistProviderLink {
        id: Uuid::now_v7(),
        artist_id,
        provider: provider.to_string(),
        external_id: external_id.to_string(),
        external_url: None,
        external_name: None,
        image_ref: None,
    };
    db::upsert_artist_provider_link(pool, &link)
        .await
        .expect("seed_artist_provider_link failed");
    link.id
}

/// Insert an album provider link and return the link id.
pub(crate) async fn seed_album_provider_link(
    pool: &SqlitePool,
    album_id: Uuid,
    provider: &str,
    external_id: &str,
) -> Uuid {
    let link = db::AlbumProviderLink {
        id: Uuid::now_v7(),
        album_id,
        provider: provider.to_string(),
        external_id: external_id.to_string(),
        external_url: None,
        external_title: None,
        cover_ref: None,
    };
    db::upsert_album_provider_link(pool, &link)
        .await
        .expect("seed_album_provider_link failed");
    link.id
}

/// Insert a download job and return it.
pub(crate) async fn seed_job(
    pool: &SqlitePool,
    album_id: Uuid,
    status: DownloadStatus,
) -> DownloadJob {
    let job = DownloadJob {
        id: Uuid::now_v7(),
        album_id,
        source: "mock".to_string(),
        album_title: "Test Album".to_string(),
        artist_name: "Test Artist".to_string(),
        status,
        quality: Quality::High,
        total_tracks: 10,
        completed_tracks: 0,
        error: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    db::insert_job(pool, &job).await.expect("seed_job failed");
    job
}

// ── Mock MetadataProvider ───────────────────────────────────────────

/// Result type for `MetadataProvider::fetch_tracks`.
type FetchTracksResult =
    Result<(Vec<ProviderTrack>, HashMap<String, serde_json::Value>), ProviderError>;

/// A configurable mock `MetadataProvider` for integration tests.
///
/// Set the `search_artists_response`, `fetch_albums_response`, etc. fields
/// before passing to a `ProviderRegistry`.
pub(crate) struct MockMetadataProvider {
    pub id: String,
    pub search_artists_result: Mutex<Result<Vec<ProviderArtist>, ProviderError>>,
    pub fetch_albums_result: Mutex<Result<Vec<ProviderAlbum>, ProviderError>>,
    pub fetch_tracks_result: Mutex<FetchTracksResult>,
    pub search_albums_result: Mutex<Result<Vec<ProviderSearchAlbum>, ProviderError>>,
    pub search_tracks_result: Mutex<Result<Vec<ProviderSearchTrack>, ProviderError>>,
}

impl MockMetadataProvider {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            search_artists_result: Mutex::new(Ok(Vec::new())),
            fetch_albums_result: Mutex::new(Ok(Vec::new())),
            fetch_tracks_result: Mutex::new(Ok((Vec::new(), HashMap::new()))),
            search_albums_result: Mutex::new(Ok(Vec::new())),
            search_tracks_result: Mutex::new(Ok(Vec::new())),
        }
    }
}

#[async_trait]
impl MetadataProvider for MockMetadataProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn display_name(&self) -> &str {
        &self.id
    }

    async fn search_artists(&self, _query: &str) -> Result<Vec<ProviderArtist>, ProviderError> {
        self.search_artists_result.lock().await.clone()
    }

    async fn fetch_albums(
        &self,
        _external_artist_id: &str,
    ) -> Result<Vec<ProviderAlbum>, ProviderError> {
        self.fetch_albums_result.lock().await.clone()
    }

    async fn fetch_tracks(
        &self,
        _external_album_id: &str,
    ) -> Result<(Vec<ProviderTrack>, HashMap<String, serde_json::Value>), ProviderError> {
        self.fetch_tracks_result.lock().await.clone()
    }

    async fn fetch_track_info_extra(
        &self,
        _external_track_id: &str,
    ) -> Option<HashMap<String, serde_json::Value>> {
        None
    }

    fn image_url(&self, image_ref: &str, size: u16) -> String {
        format!("https://mock.test/images/{image_ref}/{size}")
    }

    async fn fetch_cover_art_bytes(&self, _image_ref: &str) -> Option<Vec<u8>> {
        None
    }

    async fn search_albums(&self, _query: &str) -> Result<Vec<ProviderSearchAlbum>, ProviderError> {
        self.search_albums_result.lock().await.clone()
    }

    async fn search_tracks(&self, _query: &str) -> Result<Vec<ProviderSearchTrack>, ProviderError> {
        self.search_tracks_result.lock().await.clone()
    }
}

// ── Mock DownloadSource ─────────────────────────────────────────────

/// A configurable mock `DownloadSource` for integration tests.
#[allow(dead_code)]
pub(crate) struct MockDownloadSource {
    pub id: String,
    pub requires_linked: bool,
    pub resolve_result: Mutex<Result<PlaybackInfo, ProviderError>>,
    pub requested: Mutex<Vec<(String, Quality)>>,
}

impl MockDownloadSource {
    #[allow(dead_code)]
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            requires_linked: true,
            resolve_result: Mutex::new(Ok(PlaybackInfo::DirectUrl(
                "https://mock.test/track.flac".to_string(),
            ))),
            requested: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl DownloadSource for MockDownloadSource {
    fn id(&self) -> &str {
        &self.id
    }

    fn requires_linked_provider(&self) -> bool {
        self.requires_linked
    }

    async fn resolve_playback(
        &self,
        external_track_id: &str,
        quality: &Quality,
        _context: Option<&DownloadTrackContext>,
    ) -> Result<PlaybackInfo, ProviderError> {
        self.requested
            .lock()
            .await
            .push((external_track_id.to_string(), *quality));
        self.resolve_result.lock().await.clone()
    }
}
