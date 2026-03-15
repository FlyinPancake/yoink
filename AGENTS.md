# AGENTS.md

Guidance for coding agents working in `yoink`.

## Project Snapshot

- `yoink` is a self-hosted music library manager.
- Backend: Axum REST API with OpenAPI docs (utoipa + Scalar), Tokio, SQLx with SQLite, tracing.
- Frontend: React 19 SPA in `frontend/` — TanStack Start (SPA mode), TanStack Router/Query/DB, shadcn/ui v4, Tailwind CSS v4. Built with Bun and Vite (rolldown-vite).
- The built SPA is embedded into the server binary via `rust-embed` and served as a fallback.
- Workspace crates under `crates/`:
  - `yoink-server`: API server binary — routes, actions, providers, DB, auth, background services.
  - `yoink-shared`: shared models, error types, actions enum, helpers. Used by the server with the `ssr` feature.
- Migrations live in `crates/yoink-server/migrations/` (paired `.up.sql`/`.down.sql`).
- SQLx offline metadata is committed under `.sqlx/`.

## Setup

- Rust tools: `mise install` (installs `cargo-leptos`, `sqlx-cli`, `cargo-tarpaulin`).
- Frontend deps: `bun install` inside `frontend/`.
- Copy envs: `cp .env.example .env`.
- Backend serves on `http://127.0.0.1:3000`; frontend dev server on `http://localhost:5173` (proxies `/api/**` and `/auth/**` to the backend).

## Core Commands

### Rust — Development & Build

- Run backend in dev: `mise run dev-server` or `cargo run -p yoink-server`
- Watch mode (recompiles on change): `mise run dev-server` uses `mise watch`
- Debug build: `cargo build -p yoink-server`
- Release build (after `bun run build` in `frontend/`): `cargo build -p yoink-server --release`
- Docker: `docker compose up -d` or `docker compose -f compose.dev.yaml up -d` (with slskd)

### Rust — Lint & Format

- Format: `cargo fmt` — Check only: `cargo fmt --check`
- Lint: `mise run lint` or directly `cargo clippy --package yoink-server -- -D warnings`
- No `rustfmt.toml` or `clippy.toml` — uses defaults with `-D warnings` on CI.

### Rust — Tests

- All workspace tests: `cargo test --workspace`
- Server tests only: `cargo test -p yoink-server`
- Shared crate tests only: `cargo test -p yoink-shared`
- **Single test**: `cargo test -p yoink-server insert_and_load_artist`
- Exact name form: `cargo test -p yoink-server db::artists::tests::insert_and_load_artist -- --exact`
- List all test names: `cargo test -p yoink-server -- --list`

### Frontend

- Dev server: `bun run dev` (from `frontend/`, port 5173)
- Production build: `bun run build` (from `frontend/`)
- Lint (oxlint, type-aware): `bun run lint` — Auto-fix: `bun run lint:fix`
- Format (oxfmt): `bun run fmt` — Check only: `bun run fmt:check`
- Tests (vitest): `bun run test`
- Regenerate API types from running server: `mise run gen-frontend-types`

## Rust Conventions

### Imports and Module Layout

- Group imports: std first, external crates next, `crate::` last.
- Use nested imports for readability: `use std::{sync::Arc, time::Duration};`.
- Favor explicit imports over globs, except test helper preludes.
- Declare top-level modules in `main.rs`/`lib.rs`; keep `#[cfg(test)]` modules adjacent.
- Domain split: `actions`, `db`, `providers`, `routes`, `services`, `auth`, `models`.

### Formatting

- Follow `rustfmt` defaults; do not hand-format against the formatter.
- Trailing commas in multiline literals and calls.
- Long method chains split one call per line.
- Comment banners as section dividers are common in larger files; preserve them.
- Doc comments for public or non-obvious APIs; skip noisy comments on obvious code.

### Types and Data Modeling

- Shared domain types live in `yoink-shared` and derive `Serialize`, `Deserialize`, `ToSchema`.
- Use `Uuid` (v7) for persistent entity identifiers.
- Annotate API-facing types with `#[derive(utoipa::ToSchema)]` for OpenAPI generation.
- Annotate route handlers with `#[utoipa::path(...)]` for OpenAPI docs.
- Prefer enums over stringly typed state; use `Option<T>` for nullable/partial fields.
- Use small helper constructors on error enums instead of repeating string assembly.

### Naming

- Types/enums: `UpperCamelCase`. Functions/modules/files: `snake_case`. Constants: `SCREAMING_SNAKE_CASE`.
- Boolean fields should read naturally: `monitored`, `acquired`, `wanted`, `explicit`.
- Async functions: descriptive verb-led names like `fetch_artist_bio`, `sync_artist_albums`.

### Error Handling

- Server errors: `AppResult<T>` / `AppError` (in `crates/yoink-server/src/error.rs`).
- Shared errors: `YoinkError` (in `yoink-shared`). `AppError` converts into `YoinkError` for API responses.
- Add contextual variants with `operation`, `resource`, `reason`, `service`, `path` fields.
- Use `?` for propagation. Reserve `unwrap`/`expect` for tests, startup, or truly impossible states.
- Log recoverable failures with `tracing` instead of silently swallowing them.

### Async and Concurrency

- Tokio runtime; prefer async-first APIs in server code.
- Shared state behind `Arc<RwLock<_>>` or `Arc<Notify>`.
- Clone cheap handles (`AppState`, `reqwest::Client`, `Arc`s) instead of fighting lifetimes.
- Background loops (download workers, library reconciliation) should log failures and continue.

### Database and SQLx

- Use `sqlx::query!` / `query_as!` for typed SQLite queries.
- DB helpers return `Result<_, sqlx::Error>`; higher layers map to `AppError`.
- Schema changes need paired `.up.sql` + `.down.sql` in `crates/yoink-server/migrations/`.
- Keep `.sqlx/` metadata in sync when queries change (Docker builds use `SQLX_OFFLINE=true`).

### Logging

- Use `tracing::{debug, info, warn, error}`, never `println!`.
- Include enough context to debug provider/album/artist/job issues.
- Logging is configured in `crates/yoink-server/src/logging.rs`; respect `LOG_FORMAT` env.

### Testing

- Inline `#[cfg(test)]` modules near the owning code. Async tests use `#[tokio::test]`.
- Reuse helpers from `crates/yoink-server/src/test_helpers.rs` (`test_db`, `test_app_state`, seed helpers, mock providers).
- Use in-memory SQLite and tempdirs. Focused test names: `remove_artist_cascades`, `insert_and_load_artist`.

## Frontend Conventions

- **Routing**: TanStack Router file-based routing in `frontend/src/routes/`. Authenticated routes live under `_app/`.
- **API layer**: `openapi-fetch` + `openapi-react-query` with types generated from the server's OpenAPI spec (`frontend/src/lib/api/types.gen.ts`). Centralised queries in `queries.ts`, mutations in `mutations.ts`.
- **Real-time**: SSE connection to `/api/events` drives TanStack Query cache invalidation.
- **State**: TanStack DB collections (`frontend/src/lib/api/collections.ts`) for normalised client-side data.
- **Components**: shadcn/ui v4 primitives in `frontend/src/components/ui/`. App-level components alongside.
- **Styling**: Tailwind CSS v4. `oxfmt` sorts classes for `cn()` and `cva()` calls. Use `cn()` from `@/lib/utils` for conditional classes.
- **Path alias**: `@/*` maps to `frontend/src/*`.
- **Linting/formatting**: `oxlint` (with type-aware checking via `oxlint-tsgolint`) is the primary linter. `oxfmt` is the formatter. ESLint config exists but defers to TanStack defaults.

## Agent Do / Don't

- Do make small, local changes that match adjacent style.
- Do update tests when changing behavior.
- Do regenerate frontend types (`mise run gen-frontend-types`) after changing API routes or response shapes.
- Do annotate new routes/types with `utoipa` macros to keep the OpenAPI spec complete.
- Do preserve provider-specific fallbacks and partial-data handling.
- Don't bypass shared models/actions when wiring new API-frontend interactions.
- Don't delete `.sqlx/` metadata or migrations casually.
- Don't replace structured tracing/error types with ad-hoc strings.
- Don't mix up the two dev servers (Rust on :3000, frontend Vite on :5173).

## Suggested Validation After Changes

- Small Rust changes: `cargo fmt --check && cargo test -p <relevant-crate>`
- Server behavior changes: `cargo test -p yoink-server`
- Shared model changes: `cargo test -p yoink-shared`
- Frontend changes: `bun run lint && bun run fmt:check && bun run test` (in `frontend/`)
- API contract changes: also run `mise run gen-frontend-types` to update generated types.
- Full stack: `mise run lint && cargo test --workspace` plus frontend validation above.
- Release: `bun run build` in `frontend/`, then `cargo build -p yoink-server --release`.
