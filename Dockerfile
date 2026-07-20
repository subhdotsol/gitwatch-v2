# dependency planner
# cargo-chef generates a recipe.json that describes only the dependencies.
# This layer is invalidated only when Cargo.toml / Cargo.lock change.
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# build dependencies (cached layer)
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# build the actual binary
COPY . .
RUN cargo build --release --bin gitwatch-v2

# minimal runtime image
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/gitwatch-v2 .

EXPOSE 5000
ENTRYPOINT ["./gitwatch-v2"]
