use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Redirect, Response},
};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use sqlx::SqlitePool;

use crate::{
    db,
    error::AppError,
    redirects::{percent_encode_component, sanitize_relative_target},
};
use yoink_shared::{SearchAlbumResult, SearchArtistResult, SearchTrackResult, YoinkError};

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
    let yoink: YoinkError = err.into();
    yoink_error_response(yoink)
}

/// Convert a [`YoinkError`] into a JSON [`ApiErrorResponse`].
pub(super) fn yoink_error_response(err: YoinkError) -> ApiErrorResponse {
    ApiErrorResponse {
        error: err.to_string(),
        detail: None,
    }
}

/// Parse a raw string path parameter into a [`Uuid`].
pub(super) fn parse_uuid(raw: &str, field: &'static str) -> Result<Uuid, ApiErrorResponse> {
    raw.parse::<Uuid>().map_err(|_| ApiErrorResponse {
        error: format!("Validation failed: invalid UUID for {field}"),
        detail: Some(raw.to_string()),
    })
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

// ── Search result enrichment ────────────────────────────────────────

/// Mark each artist search result with `already_monitored` by checking
/// the `artist_provider_links` table for a matching `(provider, external_id)`.
pub(super) async fn enrich_artist_results(pool: &SqlitePool, results: &mut [SearchArtistResult]) {
    for r in results.iter_mut() {
        if let Ok(maybe_id) =
            db::find_artist_by_provider_link(pool, &r.provider, &r.external_id).await
        {
            r.already_monitored = Some(maybe_id.is_some());
        }
    }
}

/// Mark each album search result with `already_added` by checking
/// the `album_provider_links` table for a matching `(provider, external_id)`.
pub(super) async fn enrich_album_results(pool: &SqlitePool, results: &mut [SearchAlbumResult]) {
    for r in results.iter_mut() {
        if let Ok(maybe_id) =
            db::find_album_by_provider_link(pool, &r.provider, &r.external_id).await
        {
            r.already_added = Some(maybe_id.is_some());
        }
    }
}

/// Mark each track search result with `already_added` by checking
/// the `track_provider_links` table for a matching `(provider, external_id)`.
pub(super) async fn enrich_track_results(pool: &SqlitePool, results: &mut [SearchTrackResult]) {
    for r in results.iter_mut() {
        if let Ok(maybe_id) =
            db::find_track_by_provider_link(pool, &r.provider, &r.external_id).await
        {
            r.already_added = Some(maybe_id.is_some());
        }
    }
}
