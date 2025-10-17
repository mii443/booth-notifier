FROM lukemathwalker/cargo-chef:latest-rust-1.89.0 AS chef
WORKDIR app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN apt-get update && apt-get install -y --no-install-recommends libssl-dev pkg-config gcc && apt-get -y clean
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
ARG SQLX_OFFLINE=true 
RUN cargo b -r

FROM ubuntu:22.04 AS runtime
WORKDIR /booth-notifier
RUN apt-get update && apt-get install -y --no-install-recommends openssl ca-certificates libssl-dev && apt-get -y clean
COPY --from=builder /app/target/release/booth-notifier /usr/local/bin
ENTRYPOINT ["/usr/local/bin/booth-notifier"]
