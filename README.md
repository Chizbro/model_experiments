# Remote Harness

An agentic task manager with a Rust control plane, workers, CLI, and React web UI.

## Structure

- `crates/server/` — Control plane (axum, PostgreSQL)
- `crates/worker/` — Worker (git2, runs Claude Code / Cursor CLI)
- `crates/cli/` — CLI (`remote-harness`)
- `crates/api-types/` — Shared request/response types
- `web/` — Web UI (Vite + React + TypeScript)

## Build

```bash
# Rust
cargo build --workspace

# Web UI
cd web && npm install && npm run build
```

## Docs

See `docs/` for architecture, tech stack, and product documentation.
