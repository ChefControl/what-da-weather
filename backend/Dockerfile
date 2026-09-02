# Multi-stage build (DESIGN.md D8/D10): frontend dist + both Rust binaries in
# one minimal runtime image. cargo-chef caches the dependency build layer, so
# Rust dependencies recompile only when manifests change, not on every commit.
# Build context: repository root.

# ---- frontend ----
FROM node:26-alpine AS frontend
WORKDIR /ui
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# ---- rust: dependency planning & cached cook ----
FROM rust:1.98-slim AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY backend/ .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY backend/ .
RUN cargo build --release --bin weather-api --bin scheduler

# ---- runtime ----
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/weather-api /app/target/release/scheduler /usr/local/bin/
COPY --from=frontend /ui/dist /app/static
ENV STATIC_DIR=/app/static
EXPOSE 8080
# The scheduler container overrides this with `command: scheduler`.
CMD ["weather-api"]
