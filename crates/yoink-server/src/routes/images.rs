use std::time::Duration;

use axum::{
    Router,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use tracing::{debug, warn};

use crate::state::AppState;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/image/{image_id}/{size}", get(proxy_tidal_image))
        .route(
            "/api/image/{provider}/{image_id}/{size}",
            get(proxy_provider_image),
        )
}

async fn proxy_tidal_image(
    State(state): State<AppState>,
    Path((image_id, size)): Path<(String, u16)>,
) -> impl IntoResponse {
    proxy_image_impl(&state, "tidal", &image_id, size).await
}

async fn proxy_provider_image(
    State(state): State<AppState>,
    Path((provider, image_id, size)): Path<(String, String, u16)>,
) -> impl IntoResponse {
    proxy_image_impl(&state, &provider, &image_id, size).await
}

async fn proxy_image_impl(state: &AppState, provider: &str, image_id: &str, size: u16) -> Response {
    if ![160, 320, 640, 750, 1080].contains(&size) {
        debug!(
            provider,
            image_id, size, "Image proxy rejected: invalid size"
        );
        return (StatusCode::BAD_REQUEST, "invalid size").into_response();
    }

    let Some(metadata_provider) = state.registry.metadata_provider(provider) else {
        debug!(provider, image_id, "Image proxy rejected: unknown provider");
        return (StatusCode::BAD_REQUEST, "unknown provider").into_response();
    };

    if !metadata_provider.validate_image_id(image_id) {
        debug!(provider, image_id, "Image proxy rejected: invalid image id");
        return (StatusCode::BAD_REQUEST, "invalid image id").into_response();
    }

    let upstream_url = metadata_provider.image_url(image_id, size);
    debug!(provider, image_id, size, %upstream_url, "Image proxy fetching upstream");

    let response = state
        .http
        .get(&upstream_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await;

    match response {
        Ok(upstream) if upstream.status().is_success() => {
            let content_type = upstream
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("image/jpeg")
                .to_string();
            match upstream.bytes().await {
                Ok(bytes) => {
                    debug!(
                        provider,
                        image_id,
                        size,
                        bytes = bytes.len(),
                        "Image proxy success"
                    );
                    (
                        StatusCode::OK,
                        [
                            (header::CONTENT_TYPE, content_type),
                            (
                                header::CACHE_CONTROL,
                                "public, max-age=86400, immutable".to_string(),
                            ),
                        ],
                        bytes,
                    )
                        .into_response()
                }
                Err(err) => {
                    warn!(provider, image_id, %upstream_url, error = %err, "Image proxy: failed to read upstream body");
                    (StatusCode::BAD_GATEWAY, "upstream read error").into_response()
                }
            }
        }
        Ok(upstream) => {
            let status = upstream.status();
            warn!(provider, image_id, size, %upstream_url, %status, "Image proxy: upstream returned non-success");
            (StatusCode::NOT_FOUND, "image not found").into_response()
        }
        Err(err) => {
            warn!(provider, image_id, %upstream_url, error = %err, "Image proxy: upstream unreachable");
            (StatusCode::BAD_GATEWAY, "upstream unreachable").into_response()
        }
    }
}
