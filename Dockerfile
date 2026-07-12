# ── Stage 1: Frontend — build the SPA ────────────────────────
FROM docker.io/oven/bun:1 AS frontend

WORKDIR /app/frontend
COPY frontend/package.json frontend/bun.lock ./
RUN bun install --frozen-lockfile

COPY frontend/ .
RUN bun run build

# ── Stage 2: Chef — compute Rust dependency recipe ──────────
FROM docker.io/library/rust:1.97-alpine3.23 AS chef

RUN apk add --no-cache curl ca-certificates

ENV MISE_DATA_DIR="/mise"
ENV MISE_CONFIG_DIR="/mise"
ENV MISE_CACHE_DIR="/mise/cache"
ENV MISE_INSTALL_PATH="/usr/local/bin/mise"
ENV PATH="/mise/shims:$PATH"

RUN curl https://mise.run | sh

RUN mise use cargo-binstall cargo:cargo-chef

# RUN curl -fsSL https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-musl.tgz \
#     | tar -xz -C /usr/local/cargo/bin && \
#     cargo binstall cargo-chef -y

WORKDIR /app

# ── Stage 3: Planner — generate the dependency recipe ───────
FROM chef AS planner

COPY . .
RUN mise trust
RUN cargo chef prepare --recipe-path recipe.json --bin yoink-server

# ── Stage 4: Builder — cache deps, then build ───────────────
FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --bin yoink-server --recipe-path recipe.json

# Restore the real workspace tree after `cargo chef cook`, which leaves
# placeholder crate sources under `/app/crates/*`.
COPY crates/ ./crates/
COPY frontend/ ./frontend/
COPY Cargo.toml Cargo.lock ./

# Copy the frontend build output into the tree so rust-embed can pick it up.
COPY --from=frontend /app/frontend/dist/. /tmp/frontend-dist/
RUN mkdir -p frontend/dist && \
    cp -a /tmp/frontend-dist/. frontend/dist/ && \
    test -f frontend/dist/index.html

RUN cargo build --release --bin yoink-server

# ── Stage 5: Runtime ─────────────────────────────────────────
FROM docker.io/library/alpine:3.24

RUN apk add --no-cache su-exec

COPY docker-entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /app

COPY --from=builder /app/target/release/yoink-server /usr/local/bin/yoink-server

ENV PUID=1000
ENV PGID=1000
ENV MUSIC_ROOT=/music
ENV DATABASE_URL=sqlite:/data/yoink.db?mode=rwc
ENV LOG_FORMAT=pretty

EXPOSE 3000

ENTRYPOINT ["/entrypoint.sh"]
CMD ["yoink-server"]
