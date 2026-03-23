# 08 - Worker Registration & Heartbeat (Server-Side)

## Goal
Implement the server-side endpoints for worker lifecycle: registration with version checking, periodic heartbeats, stale detection, and worker management (list, get, delete).

## What to build

### Worker routes (`crates/server/src/routes/workers.rs`)

**POST /workers/register**
- Validate `client_version` — reject incompatible workers with `400 { "code": "worker_version_incompatible", "message": "..." }`
- Version check: same major.minor as server version (extract from Cargo.toml or build-time const)
- Upsert worker: insert or update if same id already exists (worker restart scenario — use `409` only if truly conflicting, or upsert per spec)
- Store: id, host, labels, capabilities, client_version, last_seen_at = now()
- Response: `201 { "worker_id": "..." }`

**POST /workers/:id/heartbeat**
- Update `last_seen_at` to now()
- Optionally store `current_job_id` for observability
- Response: `200 { "ok": true }`
- `404` if worker id unknown

**GET /workers**
- Paginated list with computed `status`: "active" if `last_seen_at` within `worker_stale_seconds`, else "stale"
- Response: `200 { "items": [{ worker_id, host, labels, status, last_seen_at }], "next_cursor": ... }`

**GET /workers/:id**
- Same shape as list item + capabilities
- `404` if not found

**DELETE /workers/:id**
- Remove worker from registry
- Reclaim any jobs assigned to this worker (set to pending, increment reclaim_count)
- Response: `204`
- `404` if not found

### Stale detection
- No background task needed in v1 — staleness is computed on read (GET /workers) and during task pull (Task 10)
- Helper function: `is_worker_stale(worker, config) -> bool` based on `last_seen_at` vs `worker_stale_seconds`

## Dependencies
- Task 04 (server foundation)
- Task 05 (API key auth — worker requests are authenticated)

## Test criteria
- [ ] `POST /workers/register` with valid version creates worker, returns `201`
- [ ] `POST /workers/register` with incompatible version returns `400` with `worker_version_incompatible`
- [ ] `POST /workers/register` with missing `client_version` logs warning but accepts (transitional)
- [ ] `POST /workers/:id/heartbeat` updates last_seen_at
- [ ] `GET /workers` returns workers with computed status (active/stale)
- [ ] Worker not heartbeating for > stale_seconds shows as "stale"
- [ ] `DELETE /workers/:id` removes worker and reclaims its jobs
- [ ] Integration tests for full registration + heartbeat cycle
- [ ] `cargo test -p server` passes
