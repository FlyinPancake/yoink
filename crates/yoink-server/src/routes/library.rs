use std::convert::Infallible;

use crate::state::AppState;
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream};
use utoipa_axum::{router::OpenApiRouter, routes};

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(sse_events))
}

/// Server-Sent Events stream
///
/// Returns a stream of server-sent events that clients can subscribe to for real-time updates.
/// The stream is implemented using a broadcast channel, allowing multiple clients to receive the same events simultaneously.
/// Each event is sent with the "update" event type and a simple "refresh" data payload,
/// which clients can use to trigger UI updates or other actions in response to changes on the server.
#[utoipa::path(
    get,
    path = "/api/events",
    tag = "Library",
    description = "Subscribe to server-sent events for real-time updates",
    responses(
        (status = 200, description = "A stream of server-sent events", content_type = "text/event-stream")
    )
)]
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
