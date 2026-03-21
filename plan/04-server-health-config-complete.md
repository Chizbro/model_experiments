# 04 — Server: health probes, base config, HTTP shell

**Status:** complete  
**Dependencies:** 01, 02 (error model in OpenAPI), 05 can run in parallel once DB URL type exists—prefer 05 first for `DATABASE_URL` pattern

## Objective

Runnable **axum** (or chosen stack) server with **no-auth** probes and structured config loading, matching [API_OVERVIEW §1](../docs/API_OVERVIEW.md#1-auth-and-base).

## Scope

**In scope**

- `GET /health` → `{ "status": "ok" }`
- `GET /ready` → ok when DB reachable / migrations done (after 05, wire readiness to migration success).
- `GET /health/idle` → idle vs work semantics per spec ([HOSTING idle / sleep](../docs/HOSTING.md)); v1 can return conservative stub until task queue exists, then implement real semantics.
- Central **config struct** (ports, DB URL, CORS origins placeholder, retention defaults, worker stale thresholds placeholders).

**Out of scope**

- Authenticated routes (task 06).

## Spec references

- [API_OVERVIEW §1](../docs/API_OVERVIEW.md#1-auth-and-base)
- [ARCHITECTURE §2a](../docs/ARCHITECTURE.md#2a-schema-migrations)

## Acceptance criteria

- Integration tests hit `/health` without API key.
- OpenAPI + handlers aligned for these routes.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test` integration or `curl` in Compose | CI |

## Implementation notes (completed)

- `ServerConfig` in `crates/server/src/config.rs`; `DATABASE_URL` optional (no ping) vs set (Postgres `SELECT 1` for `/ready`). `503` + standard error when DB ping fails.
- SQLx uses **rustls**; Compose `DATABASE_URL` includes `sslmode=disable` for the bundled Postgres.
- `/health/idle` returns conservative `idle: true` until the queue exists.
- CLI: `health`, `ready`, `idle`. Web: home page probes the three routes (optional `VITE_CONTROL_PLANE_URL`).
- Transitive `home` crate pinned in `Cargo.lock` to `0.5.11` for Rust **1.85** toolchain compatibility with sqlx.
