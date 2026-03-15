/// Ensures the frontend build output directory exists so that `rust-embed`
/// never fails to compile because of a missing folder.
///
/// In production the directory is populated by the frontend build step
/// (`bun run build` in `frontend/`).  During development the directory
/// may be empty — the server will return a helpful message instead of
/// the SPA shell.
fn main() {
    let frontend_dir = std::path::Path::new("../../frontend/.output/public");
    if !frontend_dir.exists() {
        std::fs::create_dir_all(frontend_dir)
            .expect("failed to create frontend/.output/public directory");
    }
}
