# ── Stage 1: Frontend — build the SPA ────────────────────────
FROM oven/bun:1 AS frontend

WORKDIR /app/frontend
COPY frontend/package.json frontend/bun.lock ./
RUN bun install --frozen-lockfile

COPY frontend/ .
RUN bun run build

# ── Stage 2: Chef — compute Rust dependency recipe ──────────
FROM rust:1.94 AS chef

RUN curl -fsSL https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-musl.tgz \
    | tar -xz -C /usr/local/cargo/bin && \
    cargo binstall cargo-chef -y

WORKDIR /app

# ── Stage 3: Planner — generate the dependency recipe ───────
FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 4: Builder — cache deps, then build ───────────────
FROM chef AS builder

ENV SQLX_OFFLINE=true

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --package yoink-server --recipe-path recipe.json

COPY . .

# Copy the frontend build output into the tree so rust-embed can pick it up.
COPY --from=frontend /app/frontend/.output/public/ ./frontend/.output/public/

RUN cargo build --release --package yoink-server

# ── Stage 5: Runtime ─────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update -y && \
    apt-get install -y --no-install-recommends ca-certificates gosu && \
    apt-get autoremove -y && \
    apt-get clean -y && \
    rm -rf /var/lib/apt/lists/*

COPY docker-entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /app

COPY --from=builder /app/target/release/yoink-server ./yoink-server

# better-config expects a .env file to exist
RUN touch .env

ENV PUID=1000
ENV PGID=1000
ENV MUSIC_ROOT=/music
ENV DATABASE_URL=sqlite:/data/yoink.db?mode=rwc
ENV DEFAULT_QUALITY=LOSSLESS
ENV LOG_FORMAT=pretty

EXPOSE 3000

VOLUME ["/data", "/music"]

ENTRYPOINT ["/entrypoint.sh"]
CMD ["./yoink-server"]
