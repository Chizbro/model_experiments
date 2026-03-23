# ─── Builder stage ───────────────────────────────────────────────────────────
FROM rust:1.90-bookworm AS builder

WORKDIR /app

# Copy workspace manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/api-types/Cargo.toml crates/api-types/Cargo.toml
COPY crates/server/Cargo.toml crates/server/Cargo.toml
COPY crates/worker/Cargo.toml crates/worker/Cargo.toml
COPY crates/cli/Cargo.toml crates/cli/Cargo.toml

# Create dummy source files so cargo can resolve the workspace
RUN mkdir -p crates/api-types/src && echo "// placeholder" > crates/api-types/src/lib.rs \
    && mkdir -p crates/server/src && echo "fn main() {}" > crates/server/src/main.rs && echo "// placeholder" > crates/server/src/lib.rs \
    && mkdir -p crates/worker/src && echo "fn main() {}" > crates/worker/src/main.rs \
    && mkdir -p crates/cli/src && echo "fn main() {}" > crates/cli/src/main.rs \
    && mkdir -p crates/server/migrations && touch crates/server/migrations/.keep

# Build dependencies only (cached layer)
RUN cargo build --release -p server -p worker 2>/dev/null || true

# Copy real source code
COPY crates/ crates/

# Touch source files to force rebuild of our code (not deps)
RUN touch crates/api-types/src/lib.rs \
    && touch crates/server/src/main.rs crates/server/src/lib.rs \
    && touch crates/worker/src/main.rs

# Build release binaries
RUN cargo build --release -p server -p worker

# ─── Runtime stage ───────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# ─── Install Node.js 22 LTS (needed for Claude Code CLI & Cursor Agent) ─────
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# ─── Install Claude Code CLI ────────────────────────────────────────────────
RUN npm install -g @anthropic-ai/claude-code \
    && claude --version

# ─── Install Cursor Agent CLI ────────────────────────────────────────────────
RUN curl -fsSL https://cursor.com/install | bash \
    && ln -sf "$(find /root/.local/share/cursor-agent -name cursor-agent -type f | head -1)" /usr/local/bin/cursor

WORKDIR /app

# Copy built binaries
COPY --from=builder /app/target/release/server ./server
COPY --from=builder /app/target/release/worker ./worker

# Copy migrations (server runs them on startup)
COPY crates/server/migrations/ ./migrations/

EXPOSE 3000

# Default command: run the server
CMD ["./server"]
