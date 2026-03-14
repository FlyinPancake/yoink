use axum::{Json, extract::State, response::IntoResponse};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::state::AppState;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_providers))
}

pub const TAG: &str = "Provider";

pub const TAG_DESCRIPTION: &str = "Endpoints related to metadata providers";

/// List all enabled metadata providers
///
/// Returns a list of provider IDs that are currently enabled in the system
#[utoipa::path(
    get,
    path = "/",
    tag = TAG,
    responses(
        (status = 200, description = "List of enabled metadata providers", body = Vec<String>),
    )
)]
pub(crate) async fn list_providers(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.registry.metadata_provider_ids())
}
