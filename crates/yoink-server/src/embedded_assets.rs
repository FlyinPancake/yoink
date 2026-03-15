//! Serves the statically built frontend SPA from files embedded at compile
//! time via [`rust_embed`].
//!
//! In **release builds** the files are baked into the binary.  In **debug
//! builds** they are read from disk, so a running `bun run build` in
//! `frontend/` is picked up without recompilation.
//!
//! The handler is intended to be used as the Axum router **fallback** so that
//! any request that does not match an API route is either served as a static
//! asset or falls back to the SPA shell (`_shell.html`).

use axum::{
    http::{StatusCode, Uri, header},
    response::{Html, IntoResponse, Response},
};
use rust_embed::RustEmbed;

// ── Embedded frontend assets ────────────────────────────────────────

/// The build output of the frontend SPA (`frontend/.output/public/`).
///
/// The directory is created automatically by `build.rs` if it does not
/// exist, so compilation never fails — the embed will simply be empty
/// when the frontend has not been built yet.
#[derive(RustEmbed)]
#[folder = "../../frontend/.output/public/"]
struct FrontendAssets;

/// Name of the SPA shell entry point produced by TanStack Start / Nitro.
const SPA_SHELL: &str = "_shell.html";

// ── Axum fallback handler ───────────────────────────────────────────

/// Fallback handler that serves embedded frontend assets or the SPA shell.
///
/// Resolution order:
/// 1. Exact file match (e.g. `/assets/main-xxx.js` → embedded file).
/// 2. `index.html` inside a directory path (not used by this frontend but
///    kept for completeness).
/// 3. SPA fallback → serve `_shell.html` so TanStack Router can handle
///    client-side routing.
/// 4. If the SPA shell itself is missing (frontend not built), return a
///    helpful plain-HTML message.
pub(crate) async fn serve_frontend(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // 1. Try to serve the exact file.
    if let Some(file) = FrontendAssets::get(path) {
        return serve_file(path, &file);
    }

    // 2. Try path/index.html (directory index).
    if !path.is_empty() {
        let index_path = format!("{path}/index.html");
        if let Some(file) = FrontendAssets::get(&index_path) {
            return serve_file(&index_path, &file);
        }
    }

    // 3. SPA fallback — serve the shell for any unmatched path so the
    //    client-side router can take over.
    if let Some(shell) = FrontendAssets::get(SPA_SHELL) {
        return serve_file(SPA_SHELL, &shell);
    }

    // 4. Frontend has not been built yet.
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Html(FRONTEND_NOT_BUILT_HTML),
    )
        .into_response()
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Build an HTTP response for an embedded file with appropriate headers.
fn serve_file(path: &str, file: &rust_embed::EmbeddedFile) -> Response {
    let content_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    let cache_control = if path.starts_with("assets/") {
        // Vite hashes asset filenames — they are safe to cache forever.
        "public, max-age=31536000, immutable"
    } else {
        // The SPA shell and other root files should always be revalidated.
        "no-cache"
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, cache_control.to_string()),
        ],
        file.data.to_vec(),
    )
        .into_response()
}

/// A minimal HTML page shown when the embedded frontend directory is empty
/// (i.e. the frontend has not been built).
const FRONTEND_NOT_BUILT_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>yoink</title>
  <style>
    body { font-family: system-ui, sans-serif; display: flex; align-items: center;
           justify-content: center; min-height: 100vh; margin: 0; background: #0a0a0a; color: #e5e5e5; }
    .card { text-align: center; max-width: 480px; padding: 2rem; }
    code  { background: #1a1a1a; padding: 0.2em 0.5em; border-radius: 4px; font-size: 0.9em; }
    h1    { font-size: 1.5rem; margin-bottom: 0.5rem; }
    p     { color: #a3a3a3; line-height: 1.6; }
  </style>
</head>
<body>
  <div class="card">
    <h1>Frontend not built</h1>
    <p>
      The server is running but no frontend assets were found.<br>
      Build the frontend first:
    </p>
    <p><code>cd frontend &amp;&amp; bun install &amp;&amp; bun run build</code></p>
    <p>Then restart the server.</p>
  </div>
</body>
</html>"#;
