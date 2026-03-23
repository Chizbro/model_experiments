# Remote Harness — Implementation Guide

## What this is
A self-hosted control plane + worker pool for running AI agent workflows (Claude Code, Cursor) over Git repositories. See `docs/` for full specs.

## Project structure
- `docs/` — Product specs (PRODUCT.md, ARCHITECTURE.md, API_OVERVIEW.md, etc.)
- `plan/` — Numbered feature specs. Files are `NNN-name.{pending|processing|complete}.md`
- `plan/design-system.md` — Shared architecture patterns and conventions. **Read this before any implementation.**
- `prompts/` — Agent prompts (PLANNER.md, IMPLEMENTER.md)
- `logs/` — Agent context dumps after each feature
- `crates/` — Rust workspace (server, worker, cli, api-types)
- `web/` — React SPA (Vite + TypeScript + Tailwind)

## Critical rules (from AGENTS.md)
- NEVER write stubs, TODOs, or incomplete implementations
- If adding backend functionality, it MUST be exposed in BOTH CLI and UI
- NEVER update an existing migration — always add new ones
- Keep specs and implementation in sync
- Do not cut corners — the product must be complete, useful, and consistent

## Build & test commands
- `cargo build --all-targets` — Build everything
- `cargo clippy --all-targets -- -D warnings` — Lint
- `cargo test --all-targets` — Run tests
- `cd web && npm run build` — Build web UI
- `cd web && npm run lint` — Lint web UI
- `docker compose up --build` — Full stack

## Agent workflow
1. Read `plan/design-system.md`
2. Find next `*.pending.md` spec in `plan/` (ascending order)
3. Rename to `*.processing.md`
4. Read relevant `docs/` specs referenced in the feature spec
5. Implement and test
6. Verify: `cargo build`, `cargo clippy`, `cargo test`, and web build/lint if applicable
7. Rename to `*.complete.md`
8. Dump full context to `logs/{feature-name}.log`
