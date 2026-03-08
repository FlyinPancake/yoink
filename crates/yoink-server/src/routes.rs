use std::{convert::Infallible, time::Duration};

use axum::{
    Form, Json, Router,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{
        IntoResponse, Redirect,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use serde::Deserialize;
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream};
use tracing::{debug, warn};

use uuid::Uuid;

use crate::{
    auth::{
        AuthenticatedSession, clear_session_cookie_header, extract_session_cookie,
        is_secure_request, session_cookie_header,
    },
    db,
    error::AppError,
    models::*,
    state::AppState,
};

#[derive(Debug, Deserialize)]
struct LoginForm {
    username: String,
    password: String,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CredentialsForm {
    username: String,
    #[serde(default)]
    current_password: Option<String>,
    new_password: String,
    confirm_password: String,
}

pub(crate) fn build_router(state: AppState) -> Router {
    Router::new()
        // ── API endpoints ───────────────────────────────────────
        .route("/api/auth/status", get(auth_status))
        .route("/api/library/artists", get(list_monitored_artists))
        .route("/api/library/albums", get(list_monitored_albums))
        .route("/api/downloads", get(list_download_jobs))
        .route("/api/tidal/instances", get(list_tidal_instances))
        .route("/api/albums/{album_id}/tracks", get(album_tracks))
        .route("/api/search", get(api_search))
        .route("/api/search/albums", get(api_search_albums))
        .route("/api/search/tracks", get(api_search_tracks))
        .route("/api/events", get(sse_events))
        .route("/api/image/{image_id}/{size}", get(proxy_tidal_image))
        .route(
            "/api/image/{provider}/{image_id}/{size}",
            get(proxy_provider_image),
        )
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/credentials", post(update_credentials))
        .with_state(state)
}

// ── API handlers ────────────────────────────────────────────────────

async fn auth_status(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if !state.auth.enabled() {
        return (
            StatusCode::OK,
            Json(yoink_shared::AuthStatus {
                auth_enabled: false,
                authenticated: true,
                username: None,
                must_change_password: false,
            }),
        )
            .into_response();
    }

    let cookie_value = extract_session_cookie(&headers);
    match state
        .auth
        .authenticate_request(cookie_value.as_deref(), false)
        .await
    {
        Ok(Some(session)) => (
            StatusCode::OK,
            Json(yoink_shared::AuthStatus {
                auth_enabled: true,
                authenticated: true,
                username: Some(session.username),
                must_change_password: session.must_change_password,
            }),
        )
            .into_response(),
        Ok(None) => StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            warn!(error = %err, "Failed to resolve auth status");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    if !state.auth.enabled() {
        return Redirect::to("/").into_response();
    }

    let secure = is_secure_request(&headers);
    let next = sanitize_next_target(form.next.as_deref());
    match state.auth.login(&form.username, &form.password).await {
        Ok(Some(outcome)) => {
            let redirect_target = if outcome.must_change_password {
                "/setup/password".to_string()
            } else {
                next
            };
            (
                StatusCode::SEE_OTHER,
                [
                    (
                        header::SET_COOKIE,
                        session_cookie_header(&outcome.cookie_value, secure),
                    ),
                    (header::LOCATION, redirect_target),
                ],
            )
                .into_response()
        }
        Ok(None) => redirect_with_error("/login", "Invalid username or password", Some(&next)),
        Err(err) => {
            warn!(error = %err, "Login failed unexpectedly");
            redirect_with_error("/login", "Login failed", Some(&next))
        }
    }
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let secure = is_secure_request(&headers);
    let location = if state.auth.enabled() { "/login" } else { "/" };
    if state.auth.enabled() {
        let cookie_value = extract_session_cookie(&headers);
        if let Err(err) = state.auth.logout(cookie_value.as_deref()).await {
            warn!(error = %err, "Logout failed");
        }
    }

    (
        StatusCode::SEE_OTHER,
        [
            (header::SET_COOKIE, clear_session_cookie_header(secure)),
            (header::LOCATION, location.to_string()),
        ],
    )
        .into_response()
}

async fn update_credentials(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(session): Extension<AuthenticatedSession>,
    Form(form): Form<CredentialsForm>,
) -> impl IntoResponse {
    if !state.auth.enabled() {
        return Redirect::to("/").into_response();
    }

    let secure = is_secure_request(&headers);
    let return_path = if session.must_change_password {
        "/setup/password"
    } else {
        "/settings/security"
    };
    let username = form.username.trim().to_string();

    if form.new_password != form.confirm_password {
        return redirect_with_error(return_path, "Passwords do not match", None);
    }

    if !session.must_change_password {
        let current_password = form.current_password.as_deref().unwrap_or_default();
        match state.auth.verify_current_password(current_password).await {
            Ok(true) => {}
            Ok(false) => {
                return redirect_with_error(return_path, "Current password is incorrect", None);
            }
            Err(err) => {
                warn!(error = %err, "Failed to verify current password");
                return redirect_with_error(return_path, "Failed to update credentials", None);
            }
        }
    }

    match state
        .auth
        .update_credentials(&username, &form.new_password)
        .await
    {
        Ok(outcome) => {
            let location = if session.must_change_password {
                "/".to_string()
            } else {
                "/settings/security?success=1".to_string()
            };
            (
                StatusCode::SEE_OTHER,
                [
                    (
                        header::SET_COOKIE,
                        session_cookie_header(&outcome.cookie_value, secure),
                    ),
                    (header::LOCATION, location),
                ],
            )
                .into_response()
        }
        Err(err) => {
            warn!(error = %err, "Failed to update credentials");
            redirect_with_error(return_path, credential_update_error_message(&err), None)
        }
    }
}

fn credential_update_error_message(err: &AppError) -> &str {
    match err {
        AppError::Validation { reason, .. } => reason,
        _ => "Failed to update credentials",
    }
}

async fn list_monitored_artists(State(state): State<AppState>) -> impl IntoResponse {
    let artists = state.monitored_artists.read().await.clone();
    Json(artists)
}

async fn list_monitored_albums(State(state): State<AppState>) -> impl IntoResponse {
    let albums = state.monitored_albums.read().await.clone();
    Json(albums)
}

async fn list_download_jobs(State(state): State<AppState>) -> impl IntoResponse {
    let jobs = state.download_jobs.read().await.clone();
    Json(jobs)
}

async fn list_tidal_instances(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(tidal) = state.registry.tidal_provider() {
        let payload = tidal.list_instances_payload().await;
        return Json(serde_json::to_value(payload).unwrap_or_default()).into_response();
    }
    Json(serde_json::json!({"error": "Tidal provider not available"})).into_response()
}

async fn album_tracks(
    State(state): State<AppState>,
    Path(album_id): Path<Uuid>,
) -> impl IntoResponse {
    // First try loading from local DB
    match db::load_tracks_for_album(&state.db, album_id).await {
        Ok(tracks) if !tracks.is_empty() => {
            return (StatusCode::OK, Json(tracks)).into_response();
        }
        _ => {}
    }

    // Fallback: fetch from any available metadata provider via provider link
    let links = match db::load_album_provider_links(&state.db, album_id).await {
        Ok(links) => links,
        Err(err) => {
            debug!(album_id = %album_id, error = %err, "Failed to load album provider links");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": err.to_string()})),
            )
                .into_response();
        }
    };

    // Try each provider link until one succeeds
    for link in &links {
        let Some(provider) = state.registry.metadata_provider(&link.provider) else {
            continue;
        };

        match provider.fetch_tracks(&link.external_id).await {
            Ok((provider_tracks, _album_extra)) => {
                let tracks: Vec<TrackInfo> = provider_tracks
                    .into_iter()
                    .map(|t| {
                        let secs = t.duration_secs;
                        let mins = secs / 60;
                        let rem = secs % 60;
                        TrackInfo {
                            id: Uuid::now_v7(),
                            title: t.title,
                            version: t.version,
                            disc_number: t.disc_number.unwrap_or(1),
                            track_number: t.track_number,
                            duration_secs: secs,
                            duration_display: format!("{}:{:02}", mins, rem),
                            isrc: t.isrc,
                            explicit: t.explicit,
                            quality_override: None,
                            track_artist: t.artists,
                            file_path: None,
                            monitored: false,
                            acquired: false,
                        }
                    })
                    .collect();
                return (StatusCode::OK, Json(tracks)).into_response();
            }
            Err(err) => {
                debug!(
                    album_id = %album_id,
                    provider = %link.provider,
                    error = %err,
                    "Failed to fetch tracks from provider"
                );
            }
        }
    }

    // No provider could serve the tracks
    (StatusCode::OK, Json(Vec::<TrackInfo>::new())).into_response()
}

async fn api_search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    use crate::ui::{artist_image_url, artist_profile_url};

    let q = match query.q.filter(|v| !v.trim().is_empty()) {
        Some(q) => q,
        None => return (StatusCode::OK, Json(Vec::<SearchResultArtist>::new())).into_response(),
    };

    // Check which names are already monitored
    let monitored = state.monitored_artists.read().await;
    let monitored_names: std::collections::HashSet<String> = monitored
        .iter()
        .map(|a| a.name.to_ascii_lowercase())
        .collect();
    drop(monitored);

    // Fan-out search to all providers
    let all_results = state.registry.search_artists_all(&q).await;
    let mut results = Vec::new();

    for (provider_id, artists) in all_results {
        for a in &artists {
            results.push(SearchResultArtist {
                provider: provider_id.clone(),
                external_id: a.external_id.clone(),
                name: a.name.clone(),
                image_url: artist_image_url(&provider_id, a, 160),
                url: artist_profile_url(a),
                already_monitored: monitored_names.contains(&a.name.to_ascii_lowercase()),
                disambiguation: a.disambiguation.clone(),
                artist_type: a.artist_type.clone(),
                country: a.country.clone(),
                tags: a.tags.clone(),
                popularity: a.popularity,
            });
        }
    }

    (StatusCode::OK, Json(results)).into_response()
}

async fn api_search_albums(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let q = match query.q.filter(|v| !v.trim().is_empty()) {
        Some(q) => q,
        None => return (StatusCode::OK, Json(Vec::<SearchResultAlbum>::new())).into_response(),
    };

    let all_results = state.registry.search_albums_all(&q).await;
    let mut results = Vec::new();

    for (provider_id, albums) in all_results {
        for a in albums {
            let cover_url = a
                .cover_ref
                .as_deref()
                .map(|c| yoink_shared::provider_image_url(&provider_id, c, 320));

            results.push(SearchResultAlbum {
                provider: provider_id.clone(),
                external_id: a.external_id,
                title: a.title,
                album_type: a.album_type,
                release_date: a.release_date,
                cover_url,
                url: a.url,
                explicit: a.explicit,
                artist_name: a.artist_name,
                artist_external_id: a.artist_external_id,
            });
        }
    }

    (StatusCode::OK, Json(results)).into_response()
}

async fn api_search_tracks(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let q = match query.q.filter(|v| !v.trim().is_empty()) {
        Some(q) => q,
        None => return (StatusCode::OK, Json(Vec::<SearchResultTrack>::new())).into_response(),
    };

    let all_results = state.registry.search_tracks_all(&q).await;
    let mut results = Vec::new();

    for (provider_id, tracks) in all_results {
        for t in tracks {
            let secs = t.duration_secs;
            let mins = secs / 60;
            let rem = secs % 60;

            let album_cover_url = t
                .album_cover_ref
                .as_deref()
                .map(|c| yoink_shared::provider_image_url(&provider_id, c, 160));

            results.push(SearchResultTrack {
                provider: provider_id.clone(),
                external_id: t.external_id,
                title: t.title,
                version: t.version,
                duration_secs: t.duration_secs,
                duration_display: format!("{mins}:{rem:02}"),
                isrc: t.isrc,
                explicit: t.explicit,
                artist_name: t.artist_name,
                artist_external_id: t.artist_external_id,
                album_title: t.album_title,
                album_external_id: t.album_external_id,
                album_cover_url,
            });
        }
    }

    (StatusCode::OK, Json(results)).into_response()
}

async fn sse_events(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.sse_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(()) => Some(Ok(Event::default().event("update").data("refresh"))),
        Err(_) => None, // lagged — skip
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ── Image proxy ─────────────────────────────────────────────────────

/// Legacy image proxy route: /api/image/{image_id}/{size}
/// Assumes Tidal image format for backwards compatibility.
async fn proxy_tidal_image(
    State(state): State<AppState>,
    Path((image_id, size)): Path<(String, u16)>,
) -> impl IntoResponse {
    proxy_image_impl(&state, "tidal", &image_id, size).await
}

/// Provider-aware image proxy: /api/image/{provider}/{image_id}/{size}
async fn proxy_provider_image(
    State(state): State<AppState>,
    Path((provider, image_id, size)): Path<(String, String, u16)>,
) -> impl IntoResponse {
    proxy_image_impl(&state, &provider, &image_id, size).await
}

async fn proxy_image_impl(
    state: &AppState,
    provider: &str,
    image_id: &str,
    size: u16,
) -> axum::response::Response {
    // Validate size
    if ![160, 320, 640, 750, 1080].contains(&size) {
        debug!(
            provider,
            image_id, size, "Image proxy rejected: invalid size"
        );
        return (StatusCode::BAD_REQUEST, "invalid size").into_response();
    }

    // Resolve upstream URL via the provider
    let Some(metadata_provider) = state.registry.metadata_provider(provider) else {
        debug!(provider, image_id, "Image proxy rejected: unknown provider");
        return (StatusCode::BAD_REQUEST, "unknown provider").into_response();
    };

    // Provider-specific image ID validation
    if !metadata_provider.validate_image_id(image_id) {
        debug!(provider, image_id, "Image proxy rejected: invalid image id");
        return (StatusCode::BAD_REQUEST, "invalid image id").into_response();
    }

    let upstream_url = metadata_provider.image_url(image_id, size);
    debug!(provider, image_id, size, %upstream_url, "Image proxy fetching upstream");

    let resp = state
        .http
        .get(&upstream_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await;

    match resp {
        Ok(upstream) if upstream.status().is_success() => {
            let content_type = upstream
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
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

fn redirect_with_error(base: &str, message: &str, next: Option<&str>) -> axum::response::Response {
    let mut location = format!("{base}?error={}", percent_encode_component(message));
    if let Some(next) = next.filter(|next| *next != "/") {
        location.push_str("&next=");
        location.push_str(&percent_encode_component(next));
    }
    Redirect::to(&location).into_response()
}

fn sanitize_next_target(next: Option<&str>) -> String {
    match next {
        Some(value)
            if value.starts_with('/')
                && !value.starts_with("//")
                && !value.contains('\\')
                && !value.contains("://")
                && !value
                    .chars()
                    .any(|ch| ch.is_ascii_whitespace() || ch.is_control())
                && !contains_percent_encoded_control_chars(value)
                && Uri::try_from(value)
                    .map(|uri| uri.scheme().is_none() && uri.authority().is_none())
                    .unwrap_or(false) =>
        {
            value.to_string()
        }
        _ => "/".to_string(),
    }
}

fn contains_percent_encoded_control_chars(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index + 2 < bytes.len() {
        if bytes[index] == b'%'
            && let (Some(high), Some(low)) = (
                decode_hex_digit(bytes[index + 1]),
                decode_hex_digit(bytes[index + 2]),
            )
            && ((high << 4) | low).is_ascii_control()
        {
            return true;
        }
        index += 1;
    }
    false
}

fn decode_hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn percent_encode_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::extract::{Extension, State};
    use axum::http::{HeaderMap, Request, StatusCode};
    use axum::middleware;
    use axum::response::IntoResponse;
    use axum::Form;
    use axum::{Router, routing::get as axum_get};
    use tower::ServiceExt;

    use crate::auth::{AuthenticatedSession, middleware::enforce_auth};
    use crate::db::{load_auth_settings, update_auth_settings_tx};
    use crate::models::DownloadStatus;
    use crate::providers::ProviderArtist;
    use crate::providers::registry::ProviderRegistry;
    use crate::test_helpers::*;

    use super::{CredentialsForm, build_router, sanitize_next_target, update_credentials};

    /// Helper: send a GET request to a path and return the status + body bytes.
    async fn get(state: crate::state::AppState, path: &str) -> (StatusCode, Vec<u8>) {
        let app = build_router(state);
        let req = Request::builder().uri(path).body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .to_vec();
        (status, body)
    }

    fn app_with_auth(state: crate::state::AppState) -> Router {
        build_router(state.clone()).layer(middleware::from_fn_with_state(state, enforce_auth))
    }

    async fn send(app: Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .to_vec();
        (status, headers, body)
    }

    fn session_cookie(headers: &axum::http::HeaderMap) -> String {
        headers
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .expect("missing session cookie")
            .to_string()
    }

    async fn login_cookie(state: crate::state::AppState, username: &str, password: &str) -> String {
        let req = Request::builder()
            .method("POST")
            .uri("/auth/login")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(format!(
                "username={username}&password={password}"
            )))
            .unwrap();

        let (status, headers, _) = send(app_with_auth(state), req).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        session_cookie(&headers)
    }

    // ── GET /api/library/artists ────────────────────────────────

    #[tokio::test]
    async fn list_artists_empty() {
        let (state, _tmp) = test_app_state().await;
        let (status, body) = get(state, "/api/library/artists").await;
        assert_eq!(status, StatusCode::OK);
        let artists: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(artists.is_empty());
    }

    #[tokio::test]
    async fn list_artists_with_data() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Test Artist").await;
        state.monitored_artists.write().await.push(artist.clone());

        let (status, body) = get(state, "/api/library/artists").await;
        assert_eq!(status, StatusCode::OK);
        let artists: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0]["name"], "Test Artist");
    }

    // ── GET /api/library/albums ─────────────────────────────────

    #[tokio::test]
    async fn list_albums_returns_correct_json() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let album = seed_album(&state.db, artist.id, "My Album").await;
        state.monitored_albums.write().await.push(album.clone());

        let (status, body) = get(state, "/api/library/albums").await;
        assert_eq!(status, StatusCode::OK);
        let albums: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0]["title"], "My Album");
        assert_eq!(albums[0]["monitored"], true);
    }

    // ── GET /api/downloads ──────────────────────────────────────

    #[tokio::test]
    async fn list_downloads_empty() {
        let (state, _tmp) = test_app_state().await;
        let (status, body) = get(state, "/api/downloads").await;
        assert_eq!(status, StatusCode::OK);
        let jobs: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(jobs.is_empty());
    }

    #[tokio::test]
    async fn list_downloads_with_jobs() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let album = seed_album(&state.db, artist.id, "Album").await;
        let job = seed_job(&state.db, album.id, DownloadStatus::Queued).await;
        state.download_jobs.write().await.push(job);

        let (status, body) = get(state, "/api/downloads").await;
        assert_eq!(status, StatusCode::OK);
        let jobs: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0]["status"], "queued");
    }

    // ── GET /api/albums/{id}/tracks ─────────────────────────────

    #[tokio::test]
    async fn album_tracks_from_db() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let album = seed_album(&state.db, artist.id, "Album").await;
        seed_tracks(&state.db, album.id, 3).await;

        let (status, body) = get(state, &format!("/api/albums/{}/tracks", album.id)).await;
        assert_eq!(status, StatusCode::OK);
        let tracks: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[0]["title"], "Track 1");
    }

    #[tokio::test]
    async fn album_tracks_empty_when_no_tracks() {
        let (state, _tmp) = test_app_state().await;
        let artist = seed_artist(&state.db, "Artist").await;
        let album = seed_album(&state.db, artist.id, "Album").await;

        let (status, body) = get(state, &format!("/api/albums/{}/tracks", album.id)).await;
        assert_eq!(status, StatusCode::OK);
        let tracks: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(tracks.is_empty());
    }

    // ── GET /api/search?q= ──────────────────────────────────────

    #[tokio::test]
    async fn search_empty_query_returns_empty() {
        let (state, _tmp) = test_app_state().await;
        let (status, body) = get(state, "/api/search?q=").await;
        assert_eq!(status, StatusCode::OK);
        let results: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn search_with_mock_provider_returns_results() {
        let mock = Arc::new(MockMetadataProvider::new("mock_prov"));
        *mock.search_artists_result.lock().await = Ok(vec![ProviderArtist {
            external_id: "EXT1".to_string(),
            name: "Found Artist".to_string(),
            image_ref: None,
            url: Some("https://example.com/artist".to_string()),
            disambiguation: Some("Rock band".to_string()),
            artist_type: Some("Group".to_string()),
            country: Some("US".to_string()),
            tags: vec!["rock".to_string()],
            popularity: Some(80),
        }]);

        let mut registry = ProviderRegistry::new();
        registry.register_metadata(mock as Arc<dyn crate::providers::MetadataProvider>);

        let (state, _tmp) = test_app_state_with_registry(registry).await;

        let (status, body) = get(state, "/api/search?q=Found").await;
        assert_eq!(status, StatusCode::OK);
        let results: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["name"], "Found Artist");
        assert_eq!(results[0]["provider"], "mock_prov");
        assert_eq!(results[0]["already_monitored"], false);
    }

    #[tokio::test]
    async fn search_flags_already_monitored() {
        let mock = Arc::new(MockMetadataProvider::new("mock_prov"));
        *mock.search_artists_result.lock().await = Ok(vec![ProviderArtist {
            external_id: "E1".to_string(),
            name: "Monitored One".to_string(),
            image_ref: None,
            url: None,
            disambiguation: None,
            artist_type: None,
            country: None,
            tags: vec![],
            popularity: None,
        }]);

        let mut registry = ProviderRegistry::new();
        registry.register_metadata(mock as Arc<dyn crate::providers::MetadataProvider>);

        let (state, _tmp) = test_app_state_with_registry(registry).await;

        // Add "Monitored One" to the in-memory list
        let artist = seed_artist(&state.db, "Monitored One").await;
        state.monitored_artists.write().await.push(artist);

        let (status, body) = get(state, "/api/search?q=Monitored").await;
        assert_eq!(status, StatusCode::OK);
        let results: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["already_monitored"], true);
    }

    // ── GET /api/tidal/instances ─────────────────────────────────

    #[tokio::test]
    async fn tidal_instances_no_tidal() {
        let (state, _tmp) = test_app_state().await;
        let (status, body) = get(state, "/api/tidal/instances").await;
        assert_eq!(status, StatusCode::OK);
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].is_string()); // "Tidal provider not available"
    }

    // ── GET /api/image/{provider}/{id}/{size} ────────────────────

    #[tokio::test]
    async fn image_proxy_invalid_size() {
        let mock = Arc::new(MockMetadataProvider::new("mock_prov"));
        let mut registry = ProviderRegistry::new();
        registry.register_metadata(mock as Arc<dyn crate::providers::MetadataProvider>);

        let (state, _tmp) = test_app_state_with_registry(registry).await;

        let (status, _) = get(state, "/api/image/mock_prov/abc123/999").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn image_proxy_unknown_provider() {
        let (state, _tmp) = test_app_state().await;
        let (status, _) = get(state, "/api/image/nonexistent/abc123/320").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn protected_api_requires_auth() {
        let (state, _tmp) = test_app_state_with_auth().await;
        let app = app_with_auth(state);
        let req = Request::builder()
            .uri("/api/library/artists")
            .body(Body::empty())
            .unwrap();

        let (status, _, _) = send(app, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn protected_html_redirects_to_login() {
        let (state, _tmp) = test_app_state_with_auth().await;
        let app = Router::new()
            .route("/library", axum_get(|| async { StatusCode::OK }))
            .with_state(state.clone())
            .layer(middleware::from_fn_with_state(state, enforce_auth));
        let req = Request::builder()
            .uri("/library")
            .body(Body::empty())
            .unwrap();

        let (status, headers, _) = send(app, req).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers.get("location").and_then(|v| v.to_str().ok()),
            Some("/login?next=/library")
        );
    }

    #[tokio::test]
    async fn login_sets_cookie_and_redirects_home() {
        let (state, _tmp) = test_app_state_with_auth().await;
        let app = app_with_auth(state);
        let req = Request::builder()
            .method("POST")
            .uri("/auth/login")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(
                "username=admin&password=password123&next=%2Flibrary",
            ))
            .unwrap();

        let (status, headers, _) = send(app, req).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers.get("location").and_then(|v| v.to_str().ok()),
            Some("/library")
        );
        assert!(
            headers
                .get("set-cookie")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .contains("yoink_session=")
        );
    }

    #[tokio::test]
    async fn update_credentials_reissues_session_on_success() {
        let (state, _tmp) = test_app_state_with_auth().await;
        let original_cookie = login_cookie(state.clone(), "admin", "password123").await;

        let req = Request::builder()
            .method("POST")
            .uri("/auth/credentials")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("cookie", &original_cookie)
            .body(Body::from(
                "username=root&current_password=password123&new_password=new-password&confirm_password=new-password",
            ))
            .unwrap();

        let (status, headers, _) = send(app_with_auth(state.clone()), req).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers
                .get("location")
                .and_then(|value| value.to_str().ok()),
            Some("/settings/security?success=1")
        );

        let replacement_cookie = session_cookie(&headers);
        assert_ne!(replacement_cookie, original_cookie);

        let settings = load_auth_settings(&state.db).await.unwrap().unwrap();
        assert_eq!(settings.admin_username, "root");
        assert!(!settings.must_change_password);

        let old_status_req = Request::builder()
            .uri("/api/auth/status")
            .header("cookie", &original_cookie)
            .body(Body::empty())
            .unwrap();
        let (old_status, _, _) = send(app_with_auth(state.clone()), old_status_req).await;
        assert_eq!(old_status, StatusCode::UNAUTHORIZED);

        let new_status_req = Request::builder()
            .uri("/api/auth/status")
            .header("cookie", &replacement_cookie)
            .body(Body::empty())
            .unwrap();
        let (new_status, _, body) = send(app_with_auth(state.clone()), new_status_req).await;
        assert_eq!(new_status, StatusCode::OK);

        let payload: yoink_shared::AuthStatus = serde_json::from_slice(&body).unwrap();
        assert!(payload.authenticated);
        assert_eq!(payload.username.as_deref(), Some("root"));
    }

    #[tokio::test]
    async fn update_credentials_rejects_wrong_current_password() {
        let (state, _tmp) = test_app_state_with_auth().await;
        let cookie = login_cookie(state.clone(), "admin", "password123").await;

        let req = Request::builder()
            .method("POST")
            .uri("/auth/credentials")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("cookie", &cookie)
            .body(Body::from(
                "username=root&current_password=wrong-password&new_password=new-password&confirm_password=new-password",
            ))
            .unwrap();

        let (status, headers, _) = send(app_with_auth(state.clone()), req).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers
                .get("location")
                .and_then(|value| value.to_str().ok()),
            Some("/settings/security?error=Current%20password%20is%20incorrect")
        );
        assert!(headers.get("set-cookie").is_none());

        let settings = load_auth_settings(&state.db).await.unwrap().unwrap();
        assert_eq!(settings.admin_username, "admin");

        let status_req = Request::builder()
            .uri("/api/auth/status")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let (auth_status, _, body) = send(app_with_auth(state), status_req).await;
        assert_eq!(auth_status, StatusCode::OK);

        let payload: yoink_shared::AuthStatus = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload.username.as_deref(), Some("admin"));
    }

    #[tokio::test]
    async fn update_credentials_rejects_password_confirmation_mismatch() {
        let (state, _tmp) = test_app_state_with_auth().await;
        let cookie = login_cookie(state.clone(), "admin", "password123").await;

        let req = Request::builder()
            .method("POST")
            .uri("/auth/credentials")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("cookie", &cookie)
            .body(Body::from(
                "username=root&current_password=password123&new_password=new-password&confirm_password=other-password",
            ))
            .unwrap();

        let (status, headers, _) = send(app_with_auth(state.clone()), req).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers
                .get("location")
                .and_then(|value| value.to_str().ok()),
            Some("/settings/security?error=Passwords%20do%20not%20match")
        );
        assert!(headers.get("set-cookie").is_none());

        let settings = load_auth_settings(&state.db).await.unwrap().unwrap();
        assert_eq!(settings.admin_username, "admin");

        let status_req = Request::builder()
            .uri("/api/auth/status")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let (auth_status, _, body) = send(app_with_auth(state), status_req).await;
        assert_eq!(auth_status, StatusCode::OK);

        let payload: yoink_shared::AuthStatus = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload.username.as_deref(), Some("admin"));
    }

    #[tokio::test]
    async fn update_credentials_sanitizes_internal_errors() {
        let (state, _tmp) = test_app_state_with_auth().await;
        sqlx::query("DELETE FROM auth_settings WHERE singleton = 1")
            .execute(&state.db)
            .await
            .unwrap();

        let response = update_credentials(
            State(state),
            HeaderMap::new(),
            Extension(AuthenticatedSession {
                username: "admin".to_string(),
                must_change_password: true,
            }),
            Form(CredentialsForm {
                username: "root".to_string(),
                current_password: None,
                new_password: "new-password".to_string(),
                confirm_password: "new-password".to_string(),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok()),
            Some("/setup/password?error=Failed%20to%20update%20credentials")
        );
    }

    #[tokio::test]
    async fn update_credentials_preserves_safe_validation_errors() {
        let (state, _tmp) = test_app_state_with_auth().await;

        let response = update_credentials(
            State(state),
            HeaderMap::new(),
            Extension(AuthenticatedSession {
                username: "admin".to_string(),
                must_change_password: true,
            }),
            Form(CredentialsForm {
                username: "   ".to_string(),
                current_password: None,
                new_password: "new-password".to_string(),
                confirm_password: "new-password".to_string(),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok()),
            Some("/setup/password?error=username%20cannot%20be%20empty")
        );
    }

    #[test]
    fn sanitize_next_target_rejects_header_unsafe_targets() {
        assert_eq!(sanitize_next_target(Some("/library")), "/library");
        assert_eq!(
            sanitize_next_target(Some("/library?view=grid")),
            "/library?view=grid"
        );
        assert_eq!(sanitize_next_target(Some("/\r\nLocation: /admin")), "/");
        assert_eq!(
            sanitize_next_target(Some("/library%0d%0aLocation:%20/admin")),
            "/"
        );
        assert_eq!(sanitize_next_target(Some("/library path")), "/");
        assert_eq!(sanitize_next_target(Some("/library\tpath")), "/");
    }

    #[test]
    fn sanitize_next_target_rejects_non_relative_targets() {
        assert_eq!(sanitize_next_target(Some("https://example.com")), "/");
        assert_eq!(sanitize_next_target(Some("//example.com/path")), "/");
        assert_eq!(sanitize_next_target(Some("/\\evil.example")), "/");
        assert_eq!(sanitize_next_target(Some("/://example.com")), "/");
        assert_eq!(sanitize_next_target(Some("library")), "/");
    }

    #[tokio::test]
    async fn auth_status_returns_authenticated_session() {
        let (state, _tmp) = test_app_state_with_auth().await;
        let login_app = app_with_auth(state.clone());
        let login_req = Request::builder()
            .method("POST")
            .uri("/auth/login")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("username=admin&password=password123"))
            .unwrap();
        let (_, login_headers, _) = send(login_app, login_req).await;
        let cookie = login_headers
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();

        let app = app_with_auth(state);
        let req = Request::builder()
            .uri("/api/auth/status")
            .header("cookie", cookie)
            .body(Body::empty())
            .unwrap();

        let (status, _, body) = send(app, req).await;
        assert_eq!(status, StatusCode::OK);
        let payload: yoink_shared::AuthStatus = serde_json::from_slice(&body).unwrap();
        assert!(payload.auth_enabled);
        assert!(payload.authenticated);
        assert_eq!(payload.username.as_deref(), Some("admin"));
    }

    #[tokio::test]
    async fn forced_setup_login_redirects_to_setup_page() {
        let (state, _tmp) = test_app_state_with_auth().await;
        let settings = load_auth_settings(&state.db).await.unwrap().unwrap();
        let mut tx = state.db.begin().await.unwrap();
        update_auth_settings_tx(
            &mut tx,
            &settings.admin_username,
            &settings.password_hash,
            true,
            chrono::Utc::now(),
            settings.password_changed_at,
        )
        .await
        .unwrap();

        tx.commit().await.unwrap();

        let app = app_with_auth(state);
        let req = Request::builder()
            .method("POST")
            .uri("/auth/login")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("username=admin&password=password123"))
            .unwrap();

        let (status, headers, _) = send(app, req).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers.get("location").and_then(|v| v.to_str().ok()),
            Some("/setup/password")
        );
    }

    #[tokio::test]
    async fn server_fn_path_requires_auth() {
        let (state, _tmp) = test_app_state_with_auth().await;
        let app = Router::new()
            .route("/leptos/test", axum_get(|| async { StatusCode::OK }))
            .with_state(state.clone())
            .layer(middleware::from_fn_with_state(state, enforce_auth));

        let req = Request::builder()
            .uri("/leptos/test")
            .body(Body::empty())
            .unwrap();
        let (status, _, _) = send(app, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
