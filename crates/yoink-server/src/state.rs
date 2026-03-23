use std::{path::PathBuf, sync::Arc};

use sea_orm::DatabaseConnection;
use tokio::sync::{Notify, broadcast};

use crate::app_config::AuthConfig;
use crate::{auth::AuthService, db::quality::Quality, providers::registry::ProviderRegistry};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) http: reqwest::Client,
    pub(crate) db: DatabaseConnection,
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
        let db_conn_opts = sea_orm::ConnectOptions::new(db_url);
        let conn = sea_orm::Database::connect(db_conn_opts)
            .await
            .expect("failed to connect to database");

        conn.get_schema_registry("yoink_server::db::entities::*")
            .sync(&conn)
            .await
            .expect("failed to sync database schema");

        let auth = AuthService::new(auth_config, conn.clone())
            .await
            .expect("failed to initialize authentication");

        let (sse_tx, _) = broadcast::channel(64);

        Self {
            http: reqwest::Client::new(),
            db: conn,
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
