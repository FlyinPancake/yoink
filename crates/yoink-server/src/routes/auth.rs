use axum::{
    Extension, Form, Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect},
};
use serde::Deserialize;
use tracing::warn;
use utoipa_axum::{router::OpenApiRouter, routes};
use veil::Redact;

use crate::{
    auth::{
        AuthenticatedSession, clear_session_cookie_header, extract_session_cookie,
        is_secure_request, session_cookie_header,
    },
    error::AppError,
    state::AppState,
};

use super::helpers::{redirect_with_error, sanitize_next_target};

pub(crate) const TAG: &str = "Authentication";
pub(crate) const TAG_DESCRIPTION: &str =
    "Endpoints for user authentication and credential management";

#[derive(Deserialize, Redact, utoipa::ToSchema)]
pub(super) struct LoginForm {
    pub(super) username: String,
    #[redact]
    pub(super) password: String,
    #[serde(default)]
    pub(super) next: Option<String>,
}

#[derive(Deserialize, Redact, utoipa::ToSchema)]
pub(super) struct CredentialsForm {
    pub(super) username: String,
    #[serde(default)]
    #[redact]
    pub(super) current_password: Option<String>,
    #[redact]
    pub(super) new_password: String,
    #[redact]
    pub(super) confirm_password: String,
}

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(auth_status))
        .routes(routes!(login))
        .routes(routes!(logout))
        .routes(routes!(update_credentials))
}

/// Authentication status
///
/// Returns the current authentication status of the user making the request.
/// If authentication is disabled, returns a default status indicating that authentication is not required.
/// If authentication is enabled, checks the session cookie and returns whether the user is authenticated, their username, and whether they must change their password.
#[utoipa::path(
    get,
    path = "/api/auth/status",
    tag = TAG,
    responses(
        (status = 200, description = "Authentication status retrieved successfully", body = crate::api::AuthStatus),
        (status = 401, description = "User is not authenticated"),
        (status = 500, description = "Failed to retrieve authentication status"),
    )
)]
pub(super) async fn auth_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !state.auth.enabled() {
        return (
            StatusCode::OK,
            Json(crate::api::AuthStatus {
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
            Json(crate::api::AuthStatus {
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

/// Login endpoint
///
/// Expects form data with `username`, `password`, and optional `next` fields.
/// If authentication is successful, sets a session cookie and redirects the client to the URL specified in `next`
/// (or a default page if not provided). If authentication fails, redirects back to the login page with an error message.
#[utoipa::path(
    post,
    path = "/auth/login",
    tag = TAG,
    responses(
        (status = 303, description = "Login successful, client should redirect to the URL in the Location header"),
        (status = 400, description = "Invalid login form data"),
        (status = 500, description = "Login failed due to server error"),
    )
)]
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

/// Logout endpoint
///
/// Invalidates the user's session on the server and clears the session cookie on the client.
/// Redirects to the login page if authentication is enabled, or to the home page otherwise.
#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = TAG,
    responses(
        (status = 303, description = "Logout successful, client should redirect to the URL in the Location header"),
        (status = 500, description = "Logout failed due to server error"),
    )
)]
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

/// Update credentials
///
/// Allows an authenticated user to update their username and/or password.
#[utoipa::path(
    post,
    path = "/auth/update-credentials",
    tag = TAG,
    responses(
        (status = 303, description = "Credentials updated successfully, client should redirect to the URL in the Location header"),
        (status = 400, description = "Invalid form data"),
        (status = 500, description = "Failed to update credentials due to server error"),
    )
)]
pub(super) async fn update_credentials(
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
