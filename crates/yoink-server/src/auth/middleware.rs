use axum::{
    body::Body,
    extract::{Request, State},
    http::{StatusCode, Uri},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use tracing::warn;

use crate::{
    auth::extract_session_cookie,
    error::AppError,
    redirects::{percent_encode_component, sanitize_relative_target},
    state::AppState,
};

pub(crate) async fn enforce_auth(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    if !state.auth.enabled() {
        if is_auth_only_page(&path) {
            return Redirect::to("/").into_response();
        }
        return next.run(request).await;
    }

    if is_public_path(&path) {
        if path == "/login" {
            let cookie_value = extract_session_cookie(request.headers());
            if let Ok(Some(session)) = state
                .auth
                .authenticate_request(cookie_value.as_deref(), true)
                .await
            {
                let target = if session.must_change_password {
                    "/setup/password"
                } else {
                    "/"
                };
                return Redirect::to(target).into_response();
            }
        }
        return next.run(request).await;
    }

    let cookie_value = extract_session_cookie(request.headers());
    let session = match state
        .auth
        .authenticate_request(cookie_value.as_deref(), true)
        .await
    {
        Ok(Some(session)) => session,
        Ok(None) => return unauthorized_response(request.uri(), &path),
        Err(err) => {
            warn!(error = %err, "Failed to authenticate request");
            return match err {
                AppError::Unavailable { .. } => StatusCode::SERVICE_UNAVAILABLE.into_response(),
                _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
        }
    };

    if session.must_change_password && !is_force_setup_allowed_path(&path) {
        if is_api_like_path(&path) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        return Redirect::to("/setup/password").into_response();
    }

    if !session.must_change_password && path == "/setup/password" {
        return Redirect::to("/").into_response();
    }

    request.extensions_mut().insert(session);
    next.run(request).await
}

fn unauthorized_response(uri: &Uri, path: &str) -> Response {
    if is_api_like_path(path) {
        StatusCode::UNAUTHORIZED.into_response()
    } else {
        let next = sanitize_next(uri.path_and_query().map(|value| value.as_str()));
        Redirect::to(&format!("/login?next={next}")).into_response()
    }
}

fn sanitize_next(next: Option<&str>) -> String {
    percent_encode_component(&sanitize_relative_target(next))
}

fn is_public_path(path: &str) -> bool {
    path == "/login"
        || path == "/auth/login"
        || path == "/auth/logout"
        || path == "/api/auth/status"
        || path.starts_with("/pkg/")
        || path == "/favicon.ico"
        || path == "/yoink.svg"
}

fn is_auth_only_page(path: &str) -> bool {
    matches!(path, "/login" | "/setup/password" | "/settings/security")
}

fn is_force_setup_allowed_path(path: &str) -> bool {
    matches!(
        path,
        "/setup/password" | "/auth/credentials" | "/auth/logout" | "/api/auth/status"
    ) || path.starts_with("/pkg/")
        || path == "/favicon.ico"
        || path == "/yoink.svg"
}

fn is_api_like_path(path: &str) -> bool {
    path.starts_with("/api/") || path.starts_with("/leptos/")
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
    };
    use tower::ServiceExt;

    use crate::test_helpers::test_app_state_with_auth;

    use super::{enforce_auth, sanitize_next};

    #[test]
    fn sanitize_next_rejects_external_targets() {
        assert_eq!(sanitize_next(Some("https://example.com")), "/");
        assert_eq!(sanitize_next(Some("//evil.com")), "/");
        assert_eq!(sanitize_next(Some("/\\evil.example")), "/");
        assert_eq!(sanitize_next(Some("/library%0d%0aLocation:%20/admin")), "/");
        assert_eq!(sanitize_next(Some("/library")), "/library");
    }

    #[tokio::test]
    async fn auth_status_path_bypasses_auth_middleware() {
        let (state, _tmp) = test_app_state_with_auth().await;
        let app = Router::new()
            .route("/api/auth/status", get(|| async { StatusCode::OK }))
            .with_state(state.clone())
            .layer(middleware::from_fn_with_state(state, enforce_auth));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/auth/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
