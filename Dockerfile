# syntax=docker/dockerfile:1.7
#
# Dockerfile siap Dokploy.
# - Tidak butuh DATABASE_URL saat build: stage builder menjalankan PostgreSQL
#   sementara, menerapkan sql/ + migrations/, lalu `cargo build` (sqlx macros
#   online mode memverifikasi query ke DB lokal tersebut).
# - Jika ingin memakai DB eksternal saat build, set build-arg DATABASE_URL.

############################
# Build stage
############################
FROM rust:1.91-slim-bookworm AS builder

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libpq-dev ca-certificates git postgresql postgresql-contrib

WORKDIR /app

ENV CARGO_NET_RETRY=10 \
    CARGO_HTTP_TIMEOUT=600 \
    CARGO_NET_GIT_FETCH_WITH_CLI=true \
    CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
    SQLX_OFFLINE=false

# Kosongkan = pakai PostgreSQL sementara di dalam builder (direkomendasikan)
ARG DATABASE_URL=""

# 1) Manifest dulu untuk cache dependency
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && printf 'fn main(){}' > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo fetch --locked

# 2) Aset & SQL
COPY public ./public
COPY sql    ./sql
COPY migrations ./migrations

# 3) Source
COPY src ./src

ARG BUILD_REV=dev
ARG ENABLE_WATCHER=0
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    set -e; \
    echo "Build rev: $BUILD_REV"; \
    if [ -z "$DATABASE_URL" ]; then \
      service postgresql start; \
      su postgres -c "psql -q -c \"ALTER USER postgres PASSWORD 'build'\""; \
      su postgres -c "createdb ppv_build"; \
      export DATABASE_URL=postgres://postgres:build@127.0.0.1:5432/ppv_build; \
      for f in sql/*.sql migrations/*.sql; do \
        echo "apply $f"; psql "$DATABASE_URL" -q -v ON_ERROR_STOP=1 -f "$f"; \
      done; \
    fi; \
    FEAT=""; if [ "$ENABLE_WATCHER" = "1" ]; then FEAT="--features x402-watcher"; fi; \
    touch src/main.rs; \
    cargo build --release --bins --locked $FEAT; \
    cp target/release/ppv_stream /tmp/ppv_stream; \
    cp target/release/seed_dummy /tmp/seed_dummy; \
    service postgresql stop || true

############################
# Runtime stage
############################
FROM debian:bookworm-slim AS runtime

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libpq5 libssl3 ffmpeg fonts-dejavu-core curl tini && \
    update-ca-certificates

RUN useradd -ms /bin/bash appuser
WORKDIR /app

COPY --from=builder /tmp/ppv_stream /usr/local/bin/ppv_stream
COPY --from=builder /tmp/seed_dummy /usr/local/bin/seed_dummy
COPY public     /app/public
COPY sql        /app/sql
COPY migrations /app/migrations

# Semua data persisten di /data (mount volume di sini)
RUN mkdir -p /data/storage /data/media /data/hls /data/tmp && \
    chown -R appuser:appuser /app /data

ENV PUBLIC_DIR=/app/public \
    BIND=0.0.0.0:8080 \
    STORAGE_DIR=/data/storage \
    STORAGE_LOCAL_PATH=/data/storage \
    UPLOAD_DIR=/data/storage \
    MEDIA_DIR=/data/media \
    HLS_ROOT=/data/hls \
    TMP_DIR=/data/tmp \
    WATERMARK_FONT=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf \
    RUST_LOG=info

USER appuser
EXPOSE 8080
HEALTHCHECK --interval=15s --timeout=5s --start-period=30s --retries=5 \
  CMD curl -fsS http://127.0.0.1:8080/health || exit 1

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/ppv_stream"]
