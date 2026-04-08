FROM rust:1.83 AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y libpq-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && touch src/main.rs
RUN cargo build --release --bin syncer --bin endpoint
RUN rm src/main.rs

COPY . .
RUN cargo build --release --bin syncer --bin endpoint

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y libpq5 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/syncer /app/syncer
COPY --from=builder /app/target/release/endpoint /app/endpoint

ENV RUST_LOG=info
ENV WORLD_PATH=/app/world

EXPOSE 3000

LABEL maintainer="you@example.com"