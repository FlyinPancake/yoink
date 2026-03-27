use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Redirect, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    error::{ApiError, AppError},
    redirects::{percent_encode_component, sanitize_relative_target},
};

// ── Shared JSON error envelope ──────────────────────────────────────

/// Standard JSON error body returned by API endpoints.
#[derive(Debug, Serialize, ToSchema)]
pub(super) struct ApiErrorResponse {
    pub(super) error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) detail: Option<String>,
}

impl IntoResponse for ApiErrorResponse {
    fn into_response(self) -> Response {
        let status = if self.error.starts_with("Not found") {
            StatusCode::NOT_FOUND
        } else if self.error.starts_with("Validation") {
            StatusCode::BAD_REQUEST
        } else if self.error.starts_with("Conflict") {
            StatusCode::CONFLICT
        } else if self.error.starts_with("Unauthorized") {
            StatusCode::UNAUTHORIZED
        } else if self.error.starts_with("Service unavailable") {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, Json(self)).into_response()
    }
}

/// Convert an [`AppError`] into a JSON [`ApiErrorResponse`].
pub(super) fn app_error_response(err: AppError) -> ApiErrorResponse {
    let api_error: ApiError = err.into();
    api_error_response(api_error)
}

/// Convert an [`ApiError`] into a JSON [`ApiErrorResponse`].
pub(super) fn api_error_response(err: ApiError) -> ApiErrorResponse {
    ApiErrorResponse {
        error: err.to_string(),
        detail: None,
    }
}

// ── Redirect helpers ────────────────────────────────────────────────

pub(super) fn redirect_with_error(base: &str, message: &str, next: Option<&str>) -> Response {
    let mut location = format!("{base}?error={}", percent_encode_component(message));
    if let Some(next) = next.filter(|next| *next != "/") {
        location.push_str("&next=");
        location.push_str(&percent_encode_component(next));
    }
    Redirect::to(&location).into_response()
}

pub(super) fn sanitize_next_target(next: Option<&str>) -> String {
    sanitize_relative_target(next)
}
