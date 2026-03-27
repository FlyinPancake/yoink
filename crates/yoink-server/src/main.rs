#![recursion_limit = "256"]

mod actions;
mod api;
mod app_config;
mod auth;
mod config;
mod db;
mod embedded_assets;
mod error;
mod logging;
mod providers;
mod redirects;
mod routes;
mod services;
mod state;
mod util;

use std::{sync::Arc, time::Duration};

use axum::{middleware, routing::get};
use tower::layer::Layer;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use crate::{
    app_config::AppConfig,
    config::QUALITY_WARNING,
    db::quality::Quality,
    logging::init_logging,
    providers::{
        deezer::DeezerProvider, musicbrainz::MusicBrainzProvider, registry::ProviderRegistry,
        soulseek::SoulSeekSource, tidal::TidalProvider,
    },
    routes::build_router,
    services::{download_worker_loop, reconcile_library_files},
    state::AppState,
};

#[derive(utoipa::OpenApi)]
#[openapi(
    tags(
        (name = routes::album::TAG, description = routes::album::TAG_DESCRIPTION),
        (name = routes::artist::TAG, description = routes::artist::TAG_DESCRIPTION),
        (name = routes::auth::TAG, description = routes::auth::TAG_DESCRIPTION),
        (name = routes::dashboard::TAG, description = routes::dashboard::TAG_DESCRIPTION),
        (name = routes::images::TAG, description = routes::images::TAG_DESCRIPTION),
        (name = routes::import::TAG, description = routes::import::TAG_DESCRIPTION),
        (name = routes::job::TAG, description = routes::job::TAG_DESCRIPTION),
        (name = routes::match_suggestion::TAG, description = routes::match_suggestion::TAG_DESCRIPTION),
        (name = routes::provider::TAG, description = routes::provider::TAG_DESCRIPTION),
        (name = routes::search::TAG, description = routes::search::TAG_DESCRIPTION),
        (name = routes::track::TAG, description = routes::track::TAG_DESCRIPTION),
        (name = routes::wanted::TAG, description = routes::wanted::TAG_DESCRIPTION)
    )
)]
struct ApiDoc;

#[tokio::main]
async fn main() {
    let app_config = AppConfig::from_env().unwrap_or_else(|err| {
        panic!("Failed to parse configuration from environment: {err}");
    });

    init_logging(&app_config.log_format);

    let music_root = app_config.music_root_path();
    let default_quality = app_config.default_quality;

    let quality_warning = if default_quality == Quality::Lossless {
        QUALITY_WARNING
    } else {
        ""
    };

    let db_url = app_config.database_url.clone();

    // ── Build provider registry ─────────────────────────────────
    let registry = build_registry(&app_config);

    let state = AppState::new(
        music_root.clone(),
        default_quality,
        app_config.download_lyrics,
        app_config.download_max_parallel_tracks,
        &db_url,
        registry,
        app_config.auth.clone(),
    )
    .await;

    if let Err(err) = reconcile_library_files(&state).await {
        warn!(error = %err, "Initial library reconciliation failed");
    }

    // ── Background tasks ────────────────────────────────────────
    spawn_background_tasks(&state);

    // ── Axum app ────────────────────────────────────────────────
    let (router, openapi) = build_router(state.clone()).split_for_parts();
    let openapi = ApiDoc::openapi().merge_from(openapi);
    let openapi_json = openapi.clone();
    let app = router
        .merge(Scalar::with_url("/docs", openapi))
        .route(
            "/docs/openapi.json",
            get(move || async move { axum::Json(openapi_json) }),
        )
        .fallback(embedded_assets::serve_frontend)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::middleware::enforce_auth,
        ))
        .layer(
            TraceLayer::new_for_http()
                .on_request(
                    |request: &axum::http::Request<_>, _span: &tracing::Span| {
                        debug!(method = %request.method(), uri = %request.uri(), "HTTP request started");
                    },
                )
                .on_response(
                    |response: &axum::http::Response<_>,
                     latency: Duration,
                     _span: &tracing::Span| {
                        let status = response.status().as_u16();
                        if status >= 500 {
                            error!(
                                status,
                                latency_ms = latency.as_millis(),
                                "HTTP request failed"
                            );
                        } else if status >= 400 {
                            warn!(
                                status,
                                latency_ms = latency.as_millis(),
                                "HTTP client error"
                            );
                        }
                    },
                ),
        );

    let app = NormalizePathLayer::trim_trailing_slash().layer(app);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("failed to bind to 0.0.0.0:3000");

    info!(
        bind = "0.0.0.0:3000",
        database = %db_url,
        music_root = %music_root.display(),
        default_quality = ?default_quality,
        download_lyrics = app_config.download_lyrics,
        warning = %quality_warning,
        "yoink server started"
    );
    axum::serve(
        listener,
        axum::ServiceExt::<axum::http::Request<axum::body::Body>>::into_make_service(app),
    )
    .await
    .expect("server error");
}

// ── Helpers ─────────────────────────────────────────────────────────

fn build_registry(app_config: &AppConfig) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();

    if app_config.tidal_enabled {
        let tidal = Arc::new(TidalProvider::new(
            reqwest::Client::new(),
            match app_config.tidal_api_base_url.as_str() {
                "" => None,
                ref url => Some(url.to_string()),
            },
        ));
        registry.register_metadata(Arc::clone(&tidal) as Arc<dyn providers::MetadataProvider>);
        registry.register_download(Arc::clone(&tidal) as Arc<dyn providers::DownloadSource>);
        info!("Tidal provider enabled");
    }

    if app_config.musicbrainz_enabled {
        let mb = Arc::new(MusicBrainzProvider::new());
        registry.register_metadata(Arc::clone(&mb) as Arc<dyn providers::MetadataProvider>);
        info!("MusicBrainz metadata provider enabled");
    }

    if app_config.deezer_enabled {
        let deezer = Arc::new(DeezerProvider::new());
        registry.register_metadata(Arc::clone(&deezer) as Arc<dyn providers::MetadataProvider>);
        info!("Deezer metadata provider enabled");
    }

    if app_config.soulseek_enabled {
        let soulseek = Arc::new(SoulSeekSource::new(
            reqwest::Client::new(),
            app_config.slskd_base_url.clone(),
            app_config.slskd_username.clone(),
            app_config.slskd_password.clone(),
            app_config.slskd_downloads_dir.clone(),
        ));
        registry.register_download(Arc::clone(&soulseek) as Arc<dyn providers::DownloadSource>);
        info!("SoulSeek download source enabled");
    }

    registry
}

fn spawn_background_tasks(state: &AppState) {
    let worker_state = state.clone();
    tokio::spawn(async move {
        loop {
            let worker_state = worker_state.clone();
            match download_worker_loop(worker_state).await {
                Err(err) => {
                    error!(error = %err, "Download worker loop encountered an error, restarting");
                    continue;
                }
                _ => {
                    debug!("Download worker loop exited gracefully, restarting");
                    continue;
                }
            }
        }
    });

    let reconcile_state = state.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(45));
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(err) = reconcile_library_files(&reconcile_state).await {
                warn!(error = %err, "Periodic library reconciliation failed");
            }
        }
    });
}
