# syntax=docker/dockerfile:1.7

############################
# Build stage (SQLx Online Mode)
############################
FROM rust:1.83-slim-bookworm AS builder

# ===== Install dependency OS packages =====
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libpq-dev ca-certificates git && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# ===== Stabilize Cargo networking & use sparse index =====
ENV CARGO_HTTP_MULTIPLEXING=false \
    CARGO_NET_RETRY=10 \
    CARGO_HTTP_TIMEOUT=600 \
    CARGO_NET_GIT_FETCH_WITH_CLI=true \
    CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN git config --global http.version HTTP/1.1

# ===== SQLx (online macros) =====
# NOTE: jika DB tidak bisa diakses saat build, gunakan SQLX_OFFLINE=true (lihat catatan di bawah)
ARG DATABASE_URL
ENV DATABASE_URL=${DATABASE_URL}
ENV SQLX_OFFLINE=false

# ====== Cache deps lebih stabil ======
# 1) Copy manifest dulu untuk cache layer deps
COPY Cargo.toml Cargo.lock ./

# 2) Dummy main agar `cargo fetch`/build initial bisa jalan
RUN mkdir -p src && printf 'fn main(){}' > src/main.rs

# 3) Prefetch deps
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo fetch --locked --target x86_64-unknown-linux-gnu

# 4) Copy aset yang relatif stabil (jarang berubah)
COPY public ./public
COPY sql    ./sql

# 5) Copy source asli (ini yang sering berubah)
COPY src ./src

# 6) Token anti-cache (opsional)
ARG BUILD_REV
RUN echo "Build rev: $BUILD_REV"

# 7) Build semua binary (opsional feature: watcher)
ARG ENABLE_WATCHER=0
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    FEAT=""; if [ "$ENABLE_WATCHER" = "1" ]; then FEAT="--features x402-watcher"; fi; \
    cargo build --release --bins --locked --target x86_64-unknown-linux-gnu $FEAT && \
    cp target/x86_64-unknown-linux-gnu/release/ppv_stream /tmp/ppv_stream && \
    cp target/x86_64-unknown-linux-gnu/release/seed_dummy /tmp/seed_dummy

############################
# Runtime stage
############################
FROM debian:bookworm-slim AS runtime

# ===== Install runtime dependencies =====
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libpq5 libssl3 ffmpeg fonts-dejavu-core curl && \
    rm -rf /var/lib/apt/lists/* && update-ca-certificates

# ===== User & working directory =====
RUN useradd -ms /bin/bash appuser
WORKDIR /app

# ===== Copy binaries & static files =====
COPY --from=builder /tmp/ppv_stream /usr/local/bin/ppv_stream
COPY --from=builder /tmp/seed_dummy /usr/local/bin/seed_dummy
COPY public /app/public
COPY sql    /app/sql

# ===== Direktori default untuk upload & HLS =====
RUN mkdir -p /tmp/hls /data && \
    chown -R appuser:appuser /app /tmp/hls /data && \
    chmod +x /usr/local/bin/ppv_stream /usr/local/bin/seed_dummy

# ===== Environment & runtime =====
ENV PUBLIC_DIR=/app/public \
    RUST_LOG=info

USER appuser
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/ppv_stream"]
