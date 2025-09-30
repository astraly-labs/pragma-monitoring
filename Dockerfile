# syntax=docker/dockerfile-upstream:master

FROM lukemathwalker/cargo-chef:latest-rust-slim-bullseye AS cargo-chef
WORKDIR /app

FROM cargo-chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM cargo-chef AS builder
COPY --from=planner /app/recipe.json recipe.json

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y \
    libpq-dev \
    pkg-config \
    libssl-dev \
    ca-certificates \
    wget \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

RUN cargo chef cook --profile release --recipe-path recipe.json
COPY . .
RUN cargo build --locked --release

FROM debian:bullseye-slim AS final
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y \
    libpq-dev \
    libssl1.1 \
    procps \
    && rm -rf /var/lib/apt/lists/*

ARG APP_NAME=pragma-monitoring
ENV APP_NAME $APP_NAME
COPY --from=builder /app/target/release/${APP_NAME} /bin/server
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid 10001 \
    appuser
USER appuser

EXPOSE 8080

ENV RUST_LOG=info

CMD ["/bin/server"]
