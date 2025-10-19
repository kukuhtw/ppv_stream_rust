# syntax=docker/dockerfile:1.7

############################
# Build stage (SQLx Online Mode)
############################
FROM rust:1.83-slim-bookworm AS builder

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libpq-dev ca-certificates git && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Stabilize cargo networking & use sparse index
ENV CARGO_HTTP_MULTIPLEXING=false \
    CARGO_NET_RETRY=10 \
    CARGO_HTTP_TIMEOUT=600 \
    CARGO_NET_GIT_FETCH_WITH_CLI=true \
    CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN git config --global http.version HTTP/1.1

# ⬇️ PENTING: URL DB khusus saat build (online mode untuk sqlx macros)
# Contoh value (di-passing dari compose/build arg):
#   postgres://ppv:secret@host.docker.internal:5432/ppv_stream
ARG DATABASE_URL
ENV DATABASE_URL=${DATABASE_URL}
# (opsional, supaya eksplisit)
ENV SQLX_OFFLINE=false

# --- Cache manifest & assets dulu
COPY Cargo.toml Cargo.lock ./
COPY public ./public
COPY sql ./sql

# Dummy main untuk warmup cache dependencies
RUN mkdir -p src && printf 'fn main(){}' > src/main.rs

# Prefetch deps (cache)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo fetch

# --- Copy source asli
COPY src ./src

# Token anti-cache build (opsional)
ARG BUILD_REV
RUN echo "Build rev: $BUILD_REV"

# Build semua binary (server + seeder) dalam mode release
# sqlx macros akan connect ke DB karena ENV DATABASE_URL sudah di-set
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --bins && \
    cp target/release/ppv_stream /tmp/ppv_stream && \
    cp target/release/seed_dummy /tmp/seed_dummy

############################
# Runtime stage
############################
FROM debian:bookworm-slim AS runtime

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libpq5 libssl3 ffmpeg fonts-dejavu-core curl && \
    rm -rf /var/lib/apt/lists/* && update-ca-certificates

RUN useradd -ms /bin/bash appuser
WORKDIR /app

COPY --from=builder /tmp/ppv_stream /usr/local/bin/ppv_stream
COPY --from=builder /tmp/seed_dummy /usr/local/bin/seed_dummy
COPY public /app/public
COPY sql    /app/sql

# Direktori default untuk upload & HLS
RUN mkdir -p /tmp/hls /data && \
    chown -R appuser:appuser /app /tmp/hls /data && \
    chmod +x /usr/local/bin/ppv_stream /usr/local/bin/seed_dummy

# ENV default (override via .env / compose)
ENV PUBLIC_DIR=/app/public \
    RUST_LOG=info

USER appuser
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/ppv_stream"]
