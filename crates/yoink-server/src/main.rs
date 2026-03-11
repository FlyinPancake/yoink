#![recursion_limit = "256"]

mod actions;
mod app_config;
mod auth;
mod config;
mod db;
mod error;
mod logging;
mod models;
mod providers;
mod redirects;
mod routes;
mod server_context;
mod services;
mod state;
mod ui;
mod util;

#[cfg(test)]
mod fk_regressions;
#[cfg(test)]
mod test_helpers;

use std::{sync::Arc, time::Duration};

use axum::{middleware, routing::get};
use tower::layer::Layer;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

use crate::{
    app_config::AppConfig,
    config::QUALITY_WARNING,
    logging::init_logging,
    providers::{
        deezer::DeezerProvider, musicbrainz::MusicBrainzProvider, registry::ProviderRegistry,
        soulseek::SoulSeekSource, tidal::TidalProvider,
    },
    routes::build_router,
    server_context::build_server_context,
    services::{download_worker_loop, reconcile_library_files},
    state::AppState,
};

use yoink_app::{App, shell::shell};
use yoink_shared::Quality;

#[tokio::main]
async fn main() {
    let app_config = AppConfig::from_env().unwrap_or_else(|err| {
        panic!("Failed to parse configuration from environment: {err}");
    });

    init_logging(&app_config.log_format);

    // Initialise the Leptos/Tokio executor so SSR rendering can spawn futures.
    let _leptos_routes = leptos_axum::generate_route_list(App);

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

    // ── Build Leptos server context ─────────────────────────────
    let server_ctx = build_server_context(&state);
    let site_root = app_config.leptos_site_root.clone();

    let provide_server_ctx = {
        let ctx = server_ctx.clone();
        move || {
            leptos::context::provide_context(ctx.clone());
        }
    };

    let server_fn_ctx = provide_server_ctx.clone();
    let server_fn_handler = move |req: axum::http::Request<axum::body::Body>| async move {
        leptos_axum::handle_server_fns_with_context(server_fn_ctx, req).await
    };

    let leptos_handler = || {
        let ctx = provide_server_ctx.clone();
        get(leptos_axum::render_app_to_stream_with_context(ctx, shell))
    };

    // ── Axum app ────────────────────────────────────────────────
    let app = build_router(state.clone())
        .route(
            "/leptos/{*fn_name}",
            get(server_fn_handler.clone()).post(server_fn_handler),
        )
        .nest_service("/pkg", ServeDir::new(format!("{}/pkg", site_root)))
        .nest_service(
            "/favicon.ico",
            ServeDir::new(format!("{}/favicon.ico", site_root)),
        )
        .nest_service(
            "/yoink.svg",
            ServeDir::new(format!("{}/yoink.svg", site_root)),
        )
        .fallback(leptos_handler())
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
        default_quality = %default_quality,
        download_lyrics = app_config.download_lyrics,
        warning = %quality_warning,
        "Leptos SSR app started"
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
        registry.set_tidal(Arc::clone(&tidal));
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
        download_worker_loop(worker_state).await;
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
