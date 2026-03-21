# Control plane image (dev). Worker: `Dockerfile.worker`.
FROM rust:1.85-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates

ARG RH_DOCKER_SRC_TS=0
RUN echo "rh docker src ts ${RH_DOCKER_SRC_TS}"

RUN cargo build --release -p server

FROM debian:bookworm-slim AS runtime
ARG RH_DOCKER_SRC_TS=0
RUN echo "server runtime rh docker src ts ${RH_DOCKER_SRC_TS}"
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/server /usr/local/bin/server

EXPOSE 3000
USER nobody
CMD ["server"]
