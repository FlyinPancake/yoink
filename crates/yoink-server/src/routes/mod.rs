mod auth;
mod helpers;
mod images;
mod library;

#[cfg(test)]
mod tests;

use axum::Router;

use crate::state::AppState;

pub(crate) fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(auth::router())
        .merge(library::router())
        .merge(images::router())
        .with_state(state)
}
