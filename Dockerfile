# =============================================================================
# Stage 1: Rust builder — builds server, worker, and cli binaries
# =============================================================================
FROM rust:1.87-bookworm AS rust-builder

WORKDIR /build

# Copy workspace manifests first for layer caching
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/api-types/Cargo.toml crates/api-types/Cargo.toml
COPY crates/server/Cargo.toml crates/server/Cargo.toml
COPY crates/worker/Cargo.toml crates/worker/Cargo.toml
COPY crates/cli/Cargo.toml crates/cli/Cargo.toml

# Create stub lib/main files so cargo can resolve the workspace
RUN mkdir -p crates/api-types/src && echo "" > crates/api-types/src/lib.rs \
    && mkdir -p crates/server/src && echo "fn main() {}" > crates/server/src/main.rs && touch crates/server/src/lib.rs \
    && mkdir -p crates/worker/src && echo "fn main() {}" > crates/worker/src/main.rs \
    && mkdir -p crates/cli/src && echo "fn main() {}" > crates/cli/src/main.rs

# Pre-build dependencies (cached unless Cargo.toml/lock change)
RUN cargo build --release --workspace 2>/dev/null || true

# Copy actual source code
COPY crates/ crates/

# Touch source files to invalidate the stub builds
RUN touch crates/api-types/src/lib.rs \
    crates/server/src/main.rs crates/server/src/lib.rs \
    crates/worker/src/main.rs \
    crates/cli/src/main.rs

# Build all binaries in release mode
RUN cargo build --release --workspace

# =============================================================================
# Stage 2: Web builder — builds the React SPA
# =============================================================================
FROM node:20-alpine AS web-builder

WORKDIR /build/web

COPY web/package.json web/package-lock.json ./
RUN npm ci

COPY web/ ./
RUN npm run build

# =============================================================================
# Stage 3: Server runtime
# =============================================================================
FROM debian:bookworm-slim AS server-runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=rust-builder /build/target/release/server ./server

EXPOSE 3000

ENV PORT=3000
ENV RUST_LOG=info

ENTRYPOINT ["./server"]

# =============================================================================
# Stage 4: Worker runtime
# =============================================================================
FROM debian:bookworm-slim AS worker-runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    ca-certificates \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install Cursor Agent CLI
ARG TARGETARCH
ARG CURSOR_AGENT_VERSION=2026.03.20-44cb435
RUN set -eux; \
    case "${TARGETARCH}" in \
      amd64) DL_ARCH=x64  ;; \
      arm64) DL_ARCH=arm64 ;; \
      *)     echo "unsupported TARGETARCH=${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    mkdir -p /opt/cursor-agent; \
    curl -fSL "https://downloads.cursor.com/lab/${CURSOR_AGENT_VERSION}/linux/${DL_ARCH}/agent-cli-package.tar.gz" \
      | tar -xzf - --strip-components=1 -C /opt/cursor-agent; \
    chmod -R a+rX /opt/cursor-agent; \
    ln -sf /opt/cursor-agent/cursor-agent /usr/local/bin/cursor

WORKDIR /app

COPY --from=rust-builder /build/target/release/worker ./worker

ENV RUST_LOG=info

ENTRYPOINT ["./worker"]

# =============================================================================
# Stage 5: CLI runtime (optional, for building a CLI image)
# =============================================================================
FROM debian:bookworm-slim AS cli-runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=rust-builder /build/target/release/cli ./cli

ENTRYPOINT ["./cli"]
