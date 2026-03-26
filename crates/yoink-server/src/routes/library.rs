use std::{convert::Infallible, time::Duration};

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
    let initial = tokio_stream::once(Ok(Event::default().event("connected").data("ready")));
    let updates = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(()) => Some(Ok(Event::default().event("update").data("refresh"))),
        Err(_) => None,
    });
    let stream = initial.chain(updates);

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keep-alive"),
    )
}

#[cfg(test)]
mod tests {
    use std::{str, time::Duration};

    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use tokio_stream::StreamExt as _;
    use tower::ServiceExt as _;

    use crate::{
        app_config::AuthConfig, providers::registry::ProviderRegistry, routes::build_router,
        state::AppState,
    };

    async fn test_state() -> AppState {
        let db_path = format!(
            "sqlite:/tmp/yoink-sse-test-{}.db?mode=rwc",
            uuid::Uuid::now_v7()
        );

        AppState::new(
            std::path::PathBuf::from("./music"),
            crate::db::quality::Quality::Lossless,
            false,
            1,
            &db_path,
            ProviderRegistry::new(),
            AuthConfig {
                enabled: false,
                session_secret: String::new(),
                init_admin_username: None,
                init_admin_password: None,
            },
        )
        .await
    }

    #[tokio::test]
    async fn sse_events_returns_stream_response() {
        let state = test_state().await;
        let app = build_router(state).split_for_parts().0;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/events")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static("text/event-stream"))
        );
    }

    #[tokio::test]
    async fn sse_events_emit_connected_then_refresh() {
        let state = test_state().await;
        let app = build_router(state.clone()).split_for_parts().0;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/events")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        let mut body = response.into_body().into_data_stream();

        let connected = tokio::time::timeout(Duration::from_secs(1), body.next())
            .await
            .expect("connected frame timeout")
            .expect("connected stream item")
            .expect("connected data frame");
        let connected = str::from_utf8(&connected).expect("utf8");
        assert!(connected.contains("event: connected"));
        assert!(connected.contains("data: ready"));

        state.notify_sse();

        let update = tokio::time::timeout(Duration::from_secs(1), body.next())
            .await
            .expect("update frame timeout")
            .expect("update stream item")
            .expect("update data frame");
        let update = str::from_utf8(&update).expect("utf8");
        assert!(update.contains("event: update"));
        assert!(update.contains("data: refresh"));
    }
}
