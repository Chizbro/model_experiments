# 04 - Server Foundation

## Goal
Build the axum server skeleton with configuration, database connection pool, health endpoints, standard error handling, CORS middleware, and structured logging. This is the base every server route will build on.

## What to build

### Configuration (`crates/server/src/config.rs`)
- Load from environment variables with defaults:
  - `DATABASE_URL` (required)
  - `PORT` (default 3000)
  - `CORS_ALLOWED_ORIGINS` (comma-separated, default "*" for dev)
  - `API_KEY` / `API_KEYS` (comma-separated, optional — for env-based keys)
  - `WORKER_STALE_SECONDS` (default 90)
  - `MAX_JOB_RECLAIMS` (default 3)
  - `JOB_LEASE_SECONDS` (default 0 = disabled)
  - `LOG_RETENTION_DAYS` (default 7)
  - `CHAT_HISTORY_MAX_TURNS` (default 50)

### Application state (`crates/server/src/state.rs`)
- `AppState` struct holding: `PgPool`, `Config`, any shared broadcast channels for SSE

### Health endpoints (`crates/server/src/routes/health.rs`)
- `GET /health` — `200 { "status": "ok" }` (no auth)
- `GET /ready` — `200 { "status": "ok" }` (no auth) — checks DB connectivity
- `GET /health/idle` — `200 { "idle": true }` or `503 { "idle": false, "pending_or_assigned_jobs": N }` (no auth)

### Error handling (`crates/server/src/error.rs`)
- `AppError` type implementing `IntoResponse` that produces the standard `{ "error": { "code", "message", "details" } }` JSON body
- Map common errors: 400, 401, 404, 409, 500

### CORS middleware
- Tower CORS layer configured from `CORS_ALLOWED_ORIGINS`
- Allow headers: Authorization, X-API-Key, Content-Type
- Allow methods: GET, POST, PATCH, DELETE, OPTIONS

### Structured logging
- `tracing_subscriber` with JSON format for production, pretty format for dev
- Request/response logging middleware (tower-http `TraceLayer`)

### Main entrypoint (`crates/server/src/main.rs`)
- Load config, init tracing, create DB pool, run migrations, build axum Router with health routes + middleware, bind and serve

## Dependencies
- Task 01 (repo scaffolding)
- Task 02 (api-types — for error types)
- Task 03 (database schema — for migrations to run)

## Test criteria
- [ ] Server starts with valid `DATABASE_URL` and logs "listening on 0.0.0.0:3000"
- [ ] `GET /health` returns `200 { "status": "ok" }`
- [ ] `GET /ready` returns `200` when DB is up, appropriate error when DB is down
- [ ] `GET /health/idle` returns `200 { "idle": true }` (no jobs yet)
- [ ] CORS headers present on responses when `Origin` header sent
- [ ] Invalid routes return `404` with standard error JSON
- [ ] Integration test: start server, hit health endpoints, assert responses
- [ ] `cargo test -p server` passes
