use std::{path::PathBuf, sync::Arc};

use chrono::Utc;
use sqlx::SqlitePool;
use tokio::sync::{Notify, RwLock, broadcast};
use tracing::{info, warn};

use crate::app_config::AuthConfig;
use crate::{
    auth::AuthService,
    db,
    models::{DownloadJob, MonitoredAlbum, MonitoredArtist},
    providers::registry::ProviderRegistry,
};
use yoink_shared::Quality;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) http: reqwest::Client,
    pub(crate) db: SqlitePool,
    pub(crate) monitored_artists: Arc<RwLock<Vec<MonitoredArtist>>>,
    pub(crate) monitored_albums: Arc<RwLock<Vec<MonitoredAlbum>>>,
    pub(crate) download_jobs: Arc<RwLock<Vec<DownloadJob>>>,
    pub(crate) download_notify: Arc<Notify>,
    pub(crate) sse_tx: broadcast::Sender<()>,
    pub(crate) music_root: PathBuf,
    pub(crate) default_quality: Quality,
    pub(crate) download_lyrics: bool,
    pub(crate) download_max_parallel_tracks: usize,
    pub(crate) registry: Arc<ProviderRegistry>,
    pub(crate) auth: Arc<AuthService>,
}

impl AppState {
    pub(crate) async fn new(
        music_root: PathBuf,
        default_quality: Quality,
        download_lyrics: bool,
        download_max_parallel_tracks: usize,
        db_url: &str,
        registry: ProviderRegistry,
        auth_config: AuthConfig,
    ) -> Self {
        let pool = db::open(db_url).await.expect("failed to open database");
        let auth = AuthService::new(auth_config, pool.clone())
            .await
            .expect("failed to initialize authentication");

        // Load persisted data into memory
        let artists = db::load_artists(&pool).await.unwrap_or_default();
        let albums = db::load_albums(&pool).await.unwrap_or_default();
        let jobs = db::load_jobs(&pool).await.unwrap_or_default();

        // Reset any jobs that were in-progress when we last shut down
        let mut reset_count = 0u32;
        let mut jobs_clean: Vec<DownloadJob> = Vec::with_capacity(jobs.len());
        for mut j in jobs {
            if matches!(
                j.status,
                crate::models::DownloadStatus::Resolving
                    | crate::models::DownloadStatus::Downloading
            ) {
                j.status = crate::models::DownloadStatus::Queued;
                j.updated_at = Utc::now();
                if let Err(e) = db::update_job(&pool, &j).await {
                    warn!(job_id = %j.id, error = %e, "Failed to reset stale job to queued during startup");
                }
                reset_count += 1;
            }
            jobs_clean.push(j);
        }

        // Sort jobs so newest first for UI, but worker picks oldest queued
        jobs_clean.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        info!(
            artists = artists.len(),
            albums = albums.len(),
            jobs = jobs_clean.len(),
            reset_to_queued = reset_count,
            "Loaded persisted state from database"
        );

        let (sse_tx, _) = broadcast::channel(64);

        Self {
            http: reqwest::Client::new(),
            db: pool,
            monitored_artists: Arc::new(RwLock::new(artists)),
            monitored_albums: Arc::new(RwLock::new(albums)),
            download_jobs: Arc::new(RwLock::new(jobs_clean)),
            download_notify: Arc::new(Notify::new()),
            sse_tx,
            music_root,
            default_quality,
            download_lyrics,
            download_max_parallel_tracks,
            registry: Arc::new(registry),
            auth: Arc::new(auth),
        }
    }

    /// Signal all SSE clients that state has changed.
    pub(crate) fn notify_sse(&self) {
        // Ignore error — it just means no active subscribers
        let _ = self.sse_tx.send(());
    }
}
