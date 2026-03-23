# 001: Workspace Setup & Foundation

## Goal
Create the Rust workspace, all crate skeletons, database schema, and a running server with health endpoints. After this spec, `cargo build` succeeds for all crates, the server starts, connects to Postgres, runs migrations, and responds to `/health`.

## Scope
- Workspace `Cargo.toml` at repo root
- Four crates: `api-types`, `server`, `worker`, `cli`
- Database migrations (all core tables from design-system.md §3)
- Server binary: config from env, DB pool, run migrations, bind HTTP
- Health endpoints: `GET /health`, `GET /ready`, `GET /health/idle`
- API key auth middleware (validate `Authorization: Bearer` or `X-API-Key`)
- Standard error response type (`AppError` → JSON)
- Structured logging setup (`tracing` with JSON)

## Prerequisites
- PostgreSQL running (local or Docker)
- Rust toolchain installed

## Files to create

### Root
- `Cargo.toml` — workspace definition with all deps per design-system.md §2.1 and §9
- `.env.example` — template with DATABASE_URL, API_KEY, HOST, PORT

### crates/api-types/
- `Cargo.toml` — depends on serde, serde_json, chrono, uuid
- `src/lib.rs` — All shared types: enums (SessionStatus, JobStatus, WorkflowType, AgentCli, BranchMode), request/response structs for ALL endpoints in API_OVERVIEW.md, standard error body struct. This is the single source of truth for API shapes.

### crates/server/
- `Cargo.toml` — depends on api-types (path), axum, sqlx, tokio, tower, tower-http, tracing, serde, uuid, chrono, sha2, hex, anyhow, thiserror, rand
- `migrations/20240101000000_initial_schema.sql` — All tables from design-system.md §3
- `src/main.rs` — Load config, init tracing, create DB pool, run migrations, build axum Router, bind & serve
- `src/config.rs` — `AppConfig` from env vars (DATABASE_URL, API_KEY, API_KEYS, HOST, PORT, WORKER_STALE_SECONDS, MAX_JOB_RECLAIMS, JOB_LEASE_SECONDS, CORS_ALLOWED_ORIGINS, CHAT_HISTORY_MAX_TURNS)
- `src/db.rs` — Pool creation and migration runner
- `src/error.rs` — `AppError` enum + `IntoResponse` impl (per design-system.md §2.3)
- `src/auth.rs` — Middleware: extract API key from header, validate against env keys. Skip for /health, /ready, /health/idle, /auth/* paths
- `src/state.rs` — `AppState` struct (db pool, config, broadcasters)
- `src/routes/mod.rs` — Router assembly (mount all route groups)
- `src/routes/health.rs` — GET /health, GET /ready, GET /health/idle (idle checks pending/assigned jobs count)
- `src/sse.rs` — `LogBroadcaster` and `EventBroadcaster` structs (tokio::broadcast channels) — just the infrastructure, not the endpoints yet

### crates/worker/
- `Cargo.toml` — depends on api-types (path), tokio, reqwest, serde, tracing, uuid, chrono, anyhow, git2
- `src/main.rs` — Skeleton that loads config and prints "worker starting" then exits. Just enough to compile.

### crates/cli/
- `Cargo.toml` — depends on api-types (path), tokio, reqwest, clap, serde, tracing
- `src/main.rs` — Skeleton with clap: parse `--help`, subcommand stubs. Just enough to compile.

## Acceptance criteria
1. `cargo build --all-targets` succeeds with no errors
2. `cargo clippy --all-targets -- -D warnings` passes
3. Server starts with valid DATABASE_URL and API_KEY env vars
4. `GET /health` returns `{"status":"ok"}` (200, no auth)
5. `GET /ready` returns `{"status":"ok"}` (200, no auth)
6. `GET /health/idle` returns `{"idle":true}` (200, no auth) when DB has no pending/assigned jobs
7. Any authenticated endpoint returns 401 without API key
8. Any authenticated endpoint returns 200 with valid API key
9. All tables from schema exist in Postgres after server startup
10. `cargo test` passes (at minimum, api-types compiles and serialization round-trips work)

## Implementation notes
- Use `sqlx::migrate!("./migrations")` embedded migrations
- For health/idle, query: `SELECT COUNT(*) FROM jobs WHERE status IN ('pending', 'assigned')`
- Auth middleware should be a tower Layer applied to the router, with exceptions for health/auth paths
- The api-types crate should have comprehensive types even for features not yet implemented — this crate is the contract and will be used by all other agents
