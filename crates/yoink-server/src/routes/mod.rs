pub(super) mod album;
pub(super) mod artist;
pub(super) mod auth;
pub(super) mod dashboard;
pub(super) mod helpers;
pub(super) mod images;
pub(super) mod import;
pub(super) mod job;
pub(super) mod library;
pub(super) mod match_suggestion;
pub(super) mod provider;
pub(super) mod search;
pub(super) mod track;
pub(super) mod wanted;

use utoipa_axum::router::OpenApiRouter;

use crate::state::AppState;

pub(crate) fn build_router(state: AppState) -> OpenApiRouter {
    OpenApiRouter::new()
        // Routes with full paths (no nesting needed)
        .merge(auth::router())
        .merge(images::router())
        .merge(library::router())
        // Routes with relative paths (nested under /api/<resource>)
        .nest("/api/album", album::router())
        .nest("/api/artist", artist::router())
        .nest("/api/dashboard", dashboard::router())
        .nest("/api/import", import::router())
        .nest("/api/job", job::router())
        .nest("/api/match-suggestion", match_suggestion::router())
        .nest("/api/provider", provider::router())
        .nest("/api/search", search::router())
        .nest("/api/track", track::router())
        .nest("/api/wanted", wanted::router())
        .with_state(state)
}
