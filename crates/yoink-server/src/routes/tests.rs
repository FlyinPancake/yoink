use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Extension, State};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware;
use axum::response::IntoResponse;
use axum::{Form, Router, routing::get as axum_get};
use tower::ServiceExt;

use crate::auth::{AuthenticatedSession, middleware::enforce_auth};
use crate::db::{load_auth_settings, update_auth_settings_tx};
use crate::providers::registry::ProviderRegistry;
use crate::test_helpers::*;

use super::auth::{CredentialsForm, LoginForm, update_credentials};
use super::build_router;
use super::helpers::sanitize_next_target;

/// Helper: send a GET request to a path and return the status + body bytes.
async fn get(state: crate::state::AppState, path: &str) -> (StatusCode, Vec<u8>) {
    let (app, _) = build_router(state).split_for_parts();
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
    build_router(state.clone())
        .split_for_parts()
        .0
        .layer(middleware::from_fn_with_state(state, enforce_auth))
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
        .uri("/api/events")
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
        headers
            .get("location")
            .and_then(|value| value.to_str().ok()),
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
        headers
            .get("location")
            .and_then(|value| value.to_str().ok()),
        Some("/library")
    );
    assert!(
        headers
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
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
        .uri("/auth/update-credentials")
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
        .uri("/auth/update-credentials")
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
        .uri("/auth/update-credentials")
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

#[test]
fn auth_forms_redact_passwords_in_debug_output() {
    let login = LoginForm {
        username: "admin".to_string(),
        password: "password123".to_string(),
        next: Some("/library".to_string()),
    };
    let credentials = CredentialsForm {
        username: "admin".to_string(),
        current_password: Some("current-secret".to_string()),
        new_password: "new-secret".to_string(),
        confirm_password: "confirm-secret".to_string(),
    };

    let login_debug = format!("{login:?}");
    let credentials_debug = format!("{credentials:?}");

    assert!(login_debug.contains("admin"));
    assert!(!login_debug.contains("password123"));
    assert!(!credentials_debug.contains("current-secret"));
    assert!(!credentials_debug.contains("new-secret"));
    assert!(!credentials_debug.contains("confirm-secret"));
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
        .and_then(|value| value.to_str().ok())
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
        headers
            .get("location")
            .and_then(|value| value.to_str().ok()),
        Some("/setup/password")
    );
}
