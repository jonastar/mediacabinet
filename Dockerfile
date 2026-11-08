# syntax=docker/dockerfile:1

FROM node:24-bookworm-slim AS web
WORKDIR /src/web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
# The TV app, which the server also hosts at /tv-mode/. It builds here because the
# core it shares takes its types from web/node_modules. Run from web/, pnpm is the
# version web/package.json pins.
COPY players/web/package.json players/web/pnpm-lock.yaml players/web/pnpm-workspace.yaml ../players/web/
RUN pnpm --dir ../players/web install --frozen-lockfile
COPY web/ ./
RUN pnpm build
COPY players/web/ ../players/web/
RUN pnpm --dir ../players/web build

FROM rust:1.97-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY --from=web /src/web/dist web/dist
COPY --from=web /src/players/web/dist players/web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p media-cabinet \
    && cp target/release/media-cabinet /usr/local/bin/media-cabinet

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=server /usr/local/bin/media-cabinet /usr/local/bin/media-cabinet
ENV CABINET_DATA_DIR=/data CABINET_BIND=0.0.0.0:8420
VOLUME /data
EXPOSE 8420
ENTRYPOINT ["media-cabinet"]
