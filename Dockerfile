# syntax=docker/dockerfile:1.7

# Stage 1: Build React 19 + Tailwind CSS v4 + ECharts frontend
FROM --platform=$BUILDPLATFORM node:24-bookworm-slim AS frontend-builder
WORKDIR /app
ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0
RUN corepack enable && corepack prepare pnpm@10.18.1 --activate
COPY frontend/package.json frontend/pnpm-lock.yaml ./frontend/
RUN pnpm --dir frontend install --frozen-lockfile
COPY frontend ./frontend
RUN pnpm --dir frontend build

# Stage 2: Build Rust single executable embedding frontend/dist via rust-embed
FROM rust:1-bookworm AS rust-builder
WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=frontend-builder /app/frontend/dist ./frontend/dist
RUN cargo build --locked --release -p tss-server \
    && strip /app/target/release/tss-server

# Stage 3: Minimal hardened Debian Bookworm runtime
FROM debian:bookworm-slim AS runtime
LABEL org.opencontainers.image.title="TSS Manager" \
      org.opencontainers.image.description="Modern TeamSpeak 3 / TeamSpeak 6 Operations & Governance Platform" \
      org.opencontainers.image.source="https://github.com/Cloudwhile/tss-man" \
      org.opencontainers.image.licenses="MIT"

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 tss \
    && useradd --uid 10001 --gid tss --create-home --shell /usr/sbin/nologin tss \
    && mkdir -p /app/data \
    && chown -R tss:tss /app

WORKDIR /app
COPY --from=rust-builder /app/target/release/tss-server /usr/local/bin/tss-server

USER tss:tss
VOLUME ["/app/data"]
ENV TSS_LISTEN_ADDR="0.0.0.0:3000" \
    TSS_DATA_DIR="/app/data" \
    RUST_LOG="info,tss_server=info,tss_tsproto=info"

EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:3000/api/v1/setup/status >/dev/null || exit 1

CMD ["tss-server"]
