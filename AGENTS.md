# AGENTS.md
Guidance for coding agents working in `yoink`.

## Project Overview
- `yoink` is a self-hosted music library manager.
- Backend: Rust 2024, Axum, Tokio, SeaORM, utoipa/OpenAPI, tracing.
- Frontend: React 19 SPA in `frontend/` using TanStack Start, TanStack Router, TanStack Query, TanStack DB, shadcn/ui v4, Tailwind CSS v4, Bun, and Vite.
- The frontend is built separately and embedded into the server binary via `rust-embed`.
- Workspace crates:
  - `crates/yoink-server`: API server, auth, providers, services, DB entities, background workers.

## Repo-Specific Rule Files
- Checked for Cursor rules in `.cursor/rules/` and `.cursorrules`: none found.
- Checked for Copilot rules in `.github/copilot-instructions.md`: none found.
- The only repository-local agent instructions currently live in this `AGENTS.md`.

## Setup
- Install toolchain helpers with `mise install`.
- Install frontend dependencies with `bun install` in `frontend/`.
- Copy env defaults with `cp .env.example .env` if needed.
- Backend dev server runs on `http://127.0.0.1:3000`.
- Frontend dev server runs on `http://localhost:5173` and proxies `/api/**` and `/auth/**` to the backend.

## Build, Lint, and Test Commands
### Root / Combined
- Lint everything wired through mise: `mise run lint`
- Run both dev servers through mise: `mise run dev`
- Start Docker services: `docker compose up -d`
- Start dev Docker services: `docker compose -f compose.dev.yaml up -d`

### Backend (Rust)
- Run server once: `cargo run -p yoink-server`
- Run server through mise: `mise run run-server`
- Watch mode: `mise run dev-server`
- Build debug: `cargo build -p yoink-server`
- Build release: `cargo build -p yoink-server --release`
- Format: `cargo fmt`
- Format check: `cargo fmt --check`
- Lint: `cargo clippy --package yoink-server -- -D warnings`
- Server tests: `cargo test -p yoink-server`
- Whole workspace tests: `cargo test --workspace`
- List backend tests: `cargo test -p yoink-server -- --list`
- Run tests matching a name substring: `cargo test -p yoink-server list_jobs_returns_album_and_track_jobs`
- Run one exact backend test: `cargo test -p yoink-server list_jobs_returns_album_and_track_jobs -- --exact`
- Run one exact fully-qualified backend test: `cargo test -p yoink-server services::downloads::tests::list_jobs_returns_album_and_track_jobs -- --exact`

### Frontend
- Dev server: `bun run dev`
- Production build: `bun run build`
- Preview production build: `bun run preview`
- Lint: `bun run lint`
- Auto-fix lint issues: `bun run lint:fix`
- Format: `bun run fmt`
- Format check: `bun run fmt:check`
- Run frontend tests: `bun run test`
- Run one frontend test file: `bun x vitest run src/lib/router/breadcrumbs.test.ts`
- Run one named frontend test: `bun x vitest run src/lib/router/breadcrumbs.test.ts -t "returns static breadcrumb labels"`

### API Type Generation
- Regenerate frontend API types from a running backend: `mise run gen-frontend-types`
- This writes `frontend/src/lib/api/types.gen.ts`.

## Validation Guidance
- Small Rust change: `cargo fmt --check && cargo test -p yoink-server`
- Frontend change: run `bun run lint && bun run fmt:check && bun run test` in `frontend/`
- API contract change: also run `mise run gen-frontend-types`
- Release-oriented change: run `bun run build` in `frontend/` before `cargo build -p yoink-server --release`

## Architecture Notes
- The server bootstraps from `crates/yoink-server/src/main.rs` and builds the provider registry there.
- Routing is assembled in `crates/yoink-server/src/routes/mod.rs` using `OpenApiRouter`.
- App state lives in `crates/yoink-server/src/state.rs` and holds the DB connection, provider registry, SSE broadcaster, auth service, and shared settings.
- The database is initialized through SeaORM schema sync in `AppState::new`; do not assume SQLx query macros or committed `.sqlx` metadata are in use.
- Frontend routes are file-based under `frontend/src/routes/`.
- `frontend/src/routeTree.gen.ts` and `frontend/src/lib/api/types.gen.ts` are generated; do not hand-edit them.

## Rust Style Guidelines
### Imports and Layout
- Group imports as `std` first, external crates second, `crate::` imports last.
- Prefer nested imports when it improves readability, for example `use std::{path::PathBuf, sync::Arc};`.
- Prefer explicit imports over globs.
- Keep top-level module declarations in `main.rs` or `lib.rs`.
- Keep `mod tests` adjacent to the code it tests; this repo strongly favors inline test modules.

### Formatting
- Follow `rustfmt` defaults; do not hand-format against the formatter.
- Use trailing commas in multiline enums, structs, function calls, and macro arguments.
- Split long method chains and builder chains one call per line.
- Preserve section-divider comments when present; banner comments are used in larger files.
- Add doc comments for public or non-obvious behavior, not for obvious plumbing.

### Types and Data Modeling
- Server-owned API/OpenAPI DTOs live in `crates/yoink-server/src/api/`.
- Shared models usually derive `Serialize`, `Deserialize`, and `ToSchema`.
- Use `Uuid::now_v7()` for new persistent IDs; SeaORM active models commonly set this in `ActiveModelBehavior::new()`.
- Prefer enums over raw strings for domain state such as quality, wanted status, or download status.
- Use `Option<T>` for nullable provider data and partial metadata.
- Keep conversions from DB models to shared models explicit with `impl From<...>` blocks.

### Naming
- Types, enums, and traits use `UpperCamelCase`.
- Functions, modules, files, and route helpers use `snake_case`.
- Constants use `SCREAMING_SNAKE_CASE`.
- Bool fields should read naturally, for example `monitored`, `explicit`, `authenticated`, `must_change_password`.
- Async function names are verb-led and descriptive.

### Error Handling
- Backend code uses `AppResult<T>` and `AppError` from `crates/yoink-server/src/error.rs`.
- API errors use `ApiError` in `crates/yoink-server/src/error.rs`.
- Prefer contextual error variants carrying fields like `operation`, `resource`, `reason`, `service`, or `path`.
- Prefer helper constructors such as `AppError::not_found(...)` instead of rebuilding strings at call sites.
- Use `?` for propagation.
- Reserve `unwrap` and `expect` for tests, startup, or impossible states.

### Logging, Async, and Database Patterns
- Use `tracing::{debug, info, warn, error}`; never add `println!` for application logging.
- Include useful structured context in logs, especially around providers, downloads, filesystem operations, and HTTP requests.
- Tokio is the async runtime; keep server code async-first.
- Shared state is cloned through cheap handles like `Arc`, `broadcast::Sender`, and `AppState` clones.
- This codebase uses SeaORM entities and active models, not hand-written SQLx query macros.
- Entity models live under `crates/yoink-server/src/db/entities/`; prefer `Entity::find()`, relation loaders, and active model updates that match surrounding code.
- Model timestamps are often maintained in `before_save` hooks.
- When changing API shapes, update the utoipa annotations and regenerate frontend types.

### Testing
- Prefer inline `#[cfg(test)] mod tests` blocks in the same file as the implementation.
- Async tests use `#[tokio::test]`; pure helpers often use plain `#[test]`.
- Test setup is commonly local to each module via helper functions like `test_state()` or local seed helpers.
- Use descriptive test names that explain behavior, for example `list_jobs_returns_album_and_track_jobs`.

## Frontend Style Guidelines
### General Structure
- Use file-based TanStack Router routes in `frontend/src/routes/`.
- Authenticated app routes live under `frontend/src/routes/_app/`.
- Keep route exports in the TanStack pattern: `export const Route = createFileRoute(...)(...)`.
- Router setup lives in `frontend/src/router.tsx`; root document and shared context live in `frontend/src/routes/__root.tsx`.

### Data Fetching and State
- Use the central API client in `frontend/src/lib/api/client.ts`.
- Put canonical query key helpers in `frontend/src/lib/api/queries.ts`.
- Put mutation-side cache updates and invalidation logic in `frontend/src/lib/api/mutations.ts`.
- TanStack DB collections are used for local normalized state and optimistic UI helpers.
- Prefer updating query caches intentionally instead of scattering ad hoc refetches.

### TypeScript and React
- The frontend runs in strict TypeScript mode; keep code compatible with `strict`, `noUnusedLocals`, and `noUnusedParameters`.
- Prefer `import type` for type-only imports.
- Reuse generated OpenAPI schema types from `components["schemas"][...]` when possible.
- Keep components small and focused; extract helpers for repeated view logic.
- Match existing style for props objects, inline helper functions, and early-return render branches.

### Styling and Generated Files
- Use Tailwind CSS v4 utilities and theme variables defined in `frontend/src/styles.css`.
- Use `cn()` from `@/lib/utils` for conditional classes.
- `oxfmt` sorts Tailwind classes inside `cn()` and `cva()`; avoid fighting formatter output.
- Preserve the existing design language built on shadcn/ui primitives, muted neutrals, and album-art-driven accents.
- Frontend tests use Vitest and Testing Library, typically near the code they cover.
- Do not hand-edit `frontend/src/routeTree.gen.ts` or `frontend/src/lib/api/types.gen.ts`.

## Agent Do / Don't
- Do make small, local changes that match nearby patterns.
- Do update tests when behavior changes.
- Do regenerate generated frontend API types after backend contract changes.
- Do keep OpenAPI docs complete with `#[utoipa::path(...)]` and `ToSchema` derives where needed.
- Do preserve provider-specific fallbacks and partial-data handling.
- Don't introduce alternate data-fetching patterns when a query or mutation helper already exists.
- Don't replace structured errors or tracing with plain strings.
- Don't hand-edit generated frontend files.
- Don't confuse the Rust backend port `3000` with the frontend dev server port `5173`.
