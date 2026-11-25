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
    && apt-get install -y --no-install-recommends ca-certificates libssl3 curl gosu \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 tss \
    && useradd --uid 10001 --gid tss --create-home --shell /usr/sbin/nologin tss \
    && mkdir -p /app/data \
    && chown -R tss:tss /app \
    && printf '%s\n' \
       '#!/bin/sh' \
       'set -e' \
       'DATA_DIR="${TSS_DATA_DIR:-/app/data}"' \
       'if [ "$(id -u)" = "0" ]; then' \
       '  mkdir -p "$DATA_DIR"' \
       '  chown -R "${PUID:-10001}:${PGID:-10001}" "$DATA_DIR" 2>/dev/null || true' \
       '  DB_DIR=""' \
       '  case "${DATABASE_URL:-}" in' \
       '    sqlite://*)' \
       '      DB_PATH="${DATABASE_URL#sqlite://}"' \
       '      DB_PATH="${DB_PATH%%\?*}"' \
       '      if [ -n "$DB_PATH" ] && [ "$DB_PATH" != ":memory:" ]; then' \
       '        DB_DIR="$(dirname "$DB_PATH")"' \
       '        mkdir -p "$DB_DIR"' \
       '        chown -R "${PUID:-10001}:${PGID:-10001}" "$DB_DIR" 2>/dev/null || true' \
       '      fi' \
       '      ;;' \
       '  esac' \
       '  if gosu "${PUID:-10001}:${PGID:-10001}" test -w "$DATA_DIR" && { [ -z "$DB_DIR" ] || gosu "${PUID:-10001}:${PGID:-10001}" test -w "$DB_DIR"; }; then' \
       '    exec gosu "${PUID:-10001}:${PGID:-10001}" "$@"' \
       '  fi' \
       'fi' \
       'exec "$@"' \
       > /usr/local/bin/docker-entrypoint.sh \
    && chmod +x /usr/local/bin/docker-entrypoint.sh

WORKDIR /app
COPY --from=rust-builder /app/target/release/tss-server /usr/local/bin/tss-server

VOLUME ["/app/data"]
ENV TSS_LISTEN_ADDR="0.0.0.0:3000" \
    TSS_DATA_DIR="/app/data" \
    RUST_LOG="info,tss_server=info,tss_tsproto=info"

EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:3000/api/v1/setup/status >/dev/null || exit 1

ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["tss-server"]
