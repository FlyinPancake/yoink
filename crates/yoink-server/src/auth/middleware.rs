use axum::{
    body::Body,
    extract::{Request, State},
    http::{StatusCode, Uri},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};

use crate::{auth::extract_session_cookie, state::AppState};

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
        Ok(session) => session,
        Err(_) => None,
    };

    let Some(session) = session else {
        return unauthorized_response(request.uri(), &path);
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
    let Some(next) = next else {
        return "%2F".to_string();
    };
    if !next.starts_with('/') || next.starts_with("//") {
        return "%2F".to_string();
    }
    percent_encode_component(next)
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

fn is_public_path(path: &str) -> bool {
    path == "/login"
        || path == "/auth/login"
        || path == "/auth/logout"
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
    use super::sanitize_next;

    #[test]
    fn sanitize_next_rejects_external_targets() {
        assert_eq!(sanitize_next(Some("https://example.com")), "%2F");
        assert_eq!(sanitize_next(Some("//evil.com")), "%2F");
        assert_eq!(sanitize_next(Some("/library")), "/library");
    }
}
