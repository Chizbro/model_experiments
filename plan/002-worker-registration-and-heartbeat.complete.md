# 002: Worker Registration & Heartbeat

## Goal
Workers can register with the control plane and send periodic heartbeats. The server tracks worker status and marks workers stale when heartbeats stop.

## Scope
### Server endpoints
- `POST /workers/register` — Create/update worker in DB. Validate `client_version` (reject with `worker_version_incompatible` if incompatible). Return 201 with worker_id. Handle 409 for duplicate ID (upsert: update existing registration).
- `POST /workers/:id/heartbeat` — Update `last_seen_at`, accept status and current_job_id. Return 200. Return 404 if worker unknown.
- `GET /workers` — List all workers (paginated per API_OVERVIEW §3).
- `GET /workers/:id` — Get single worker. 404 if not found.
- `DELETE /workers/:id` — Remove worker from registry. Also set any jobs assigned to this worker back to pending (increment reclaim_count). Return 204. 404 if not found.

### Server background task
- Stale detection: periodic task (e.g., every 30s) that marks workers as `stale` when `NOW() - last_seen_at > worker_stale_seconds` (from config, default 90s). Stale workers are not assigned new tasks.

### Server route file
- `src/routes/workers.rs` — All worker endpoints

## Prerequisites
- Spec 001 complete (workspace, DB, server skeleton)

## Files to create/modify
- `crates/server/src/routes/workers.rs` — New file with all worker endpoints
- `crates/server/src/routes/mod.rs` — Mount worker routes
- `crates/server/src/main.rs` — Spawn stale-detection background task

## Acceptance criteria
1. `POST /workers/register` with valid body → 201 + worker_id
2. `POST /workers/register` with same ID again → upsert (200 or 201), worker updated
3. `POST /workers/register` without `client_version` → accepted with warning log
4. `POST /workers/:id/heartbeat` → 200, last_seen_at updated
5. `POST /workers/:id/heartbeat` for unknown ID → 404
6. `GET /workers` → 200 with list of registered workers
7. `GET /workers/:id` → 200 with worker details
8. `DELETE /workers/:id` → 204, worker removed
9. After `worker_stale_seconds` without heartbeat, worker status becomes `stale`
10. `DELETE /workers/:id` with assigned jobs → jobs return to pending
11. `cargo test` passes — write at least 3 integration tests (register, heartbeat, stale detection)
12. `cargo clippy` clean

## Implementation notes
- For version compatibility: v1 just checks that client_version is present and non-empty. Strict semver range checking is future work.
- Stale detection runs in a `tokio::spawn` loop with `tokio::time::interval`.
- Pagination: cursor is the worker's `created_at` timestamp (or id); order by created_at DESC.
