# Ñyokot — multi-stage build: cached deps (cargo-chef) + static musl binary
# + distroless runtime. Final image carries no shell, no package manager.
ARG RUST_VERSION=1.98

FROM lukemathwalker/cargo-chef:latest-rust-${RUST_VERSION} AS chef
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update && apt-get install -y --no-install-recommends musl-tools && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-musl
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM gcr.io/distroless/static-debian12:nonroot AS runtime
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/nyokot /nyokot
USER 65532:65532
ENTRYPOINT ["/nyokot"]
