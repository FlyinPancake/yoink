use leptos::prelude::*;

use yoink_shared::ServerAction;

/// Extract the [`ServerContext`](yoink_shared::ServerContext) from the Leptos
/// context, returning a consistent `ServerFnError` when it is missing.
///
/// Every `#[server]` function should call this instead of manually writing
/// `use_context::<ServerContext>().ok_or_else(|| ...)`.
#[cfg(feature = "ssr")]
pub fn require_ctx() -> Result<yoink_shared::ServerContext, ServerFnError> {
    use_context::<yoink_shared::ServerContext>()
        .ok_or_else(|| ServerFnError::new("ServerContext not available"))
}

/// Dispatch a user action to the server.
///
/// Called from the WASM client via `spawn_local`. The server executes the
/// action (mutates state, writes to DB, enqueues downloads, etc.) and fires
/// an SSE notification so all connected clients refresh.
#[server(
    name = DispatchAction,
    prefix = "/leptos",
    input = server_fn::codec::Json,
    output = server_fn::codec::Json
)]
pub async fn dispatch_action(action: ServerAction) -> Result<(), ServerFnError> {
    let ctx = require_ctx()?;

    (ctx.dispatch_action)(action)
        .await
        .map_err(ServerFnError::new)
}
