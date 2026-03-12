use std::convert::Infallible;

use crate::state::AppState;
use axum::{
    Router,
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream};

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/api/events", get(sse_events))
}

async fn sse_events(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.sse_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(()) => Some(Ok(Event::default().event("update").data("refresh"))),
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
