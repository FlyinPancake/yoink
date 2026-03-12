pub(super) mod auth;
pub(super) mod helpers;
pub(super) mod images;
pub(super) mod library;

#[cfg(test)]
mod tests;

use utoipa_axum::router::OpenApiRouter;

use crate::state::AppState;

pub(crate) fn build_router(state: AppState) -> OpenApiRouter {
    OpenApiRouter::new()
        .merge(auth::router())
        .merge(library::router())
        .merge(images::router())
        .with_state(state)
}
