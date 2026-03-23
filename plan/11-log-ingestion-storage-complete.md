# 11 - Log Ingestion, Storage & Retention

## Goal
Implement server-side log handling: receiving log batches from workers, storing in PostgreSQL, serving paginated log history, manual deletion, and automated retention cleanup.

## What to build

### Log routes (`crates/server/src/routes/logs.rs`)

**POST /workers/tasks/:id/logs**
- Accept batch of log entries from worker
- Enrich each entry with session_id, job_id, worker_id from task context
- Bulk insert into `logs` table
- Also write to local log file (dual-write: structured JSON, one file per session or rotating)
- Response: `202 { "accepted": true }`

**GET /sessions/:id/logs**
- Paginated log history for a session
- Query params: `limit`, `cursor`, optional `job_id`, `level`, `last` (for tail mode — last N entries)
- Order by timestamp ASC (oldest first for full history), or DESC then reverse for `last` mode
- Response: `200 { "items": [LogEntry], "next_cursor": ... }`
- `404` if session not found

**DELETE /sessions/:id/logs**
- Optional query: `job_id` — delete logs for that job only, or all session logs if omitted
- Response: `204`
- `404` if session (or job) not found

### Control plane self-logging
- Server logs its own structured events to the same `logs` table (source: "control_plane")
- Key events: session created, job assigned, job completed, worker registered, worker stale
- Also writes to local log files via tracing

### Local file logging (dual-write)
- Configure log directory (env: `LOG_DIR`, default `./logs/`)
- Write structured JSON log lines to files, rotated by size or time
- One file per day or per session (implementation-defined)

### Retention cleanup
- Background task (tokio::spawn interval, e.g. every hour)
- Delete logs older than `LOG_RETENTION_DAYS` where session is not `retain_forever`
- Respect job-level `retain_forever` (column TBD — may need schema update)
- Log cleanup activity

### Cursor-based pagination helper
- Generic pagination utility for logs (and reusable for sessions, workers, etc.)
- Encode cursor as base64 of (timestamp, id) for stable pagination

## Dependencies
- Task 04 (server foundation)
- Task 09 (sessions — logs reference sessions)

## Test criteria
- [ ] `POST /workers/tasks/:id/logs` accepts batch and stores in DB
- [ ] `GET /sessions/:id/logs` returns paginated logs in chronological order
- [ ] `GET /sessions/:id/logs?job_id=X` filters to specific job
- [ ] `GET /sessions/:id/logs?level=error` filters by level
- [ ] `GET /sessions/:id/logs?last=50` returns last 50 entries
- [ ] Cursor pagination works correctly across pages (no duplicates, no gaps)
- [ ] `DELETE /sessions/:id/logs` removes all logs for session
- [ ] `DELETE /sessions/:id/logs?job_id=X` removes only that job's logs
- [ ] Retention cleanup deletes old logs but preserves `retain_forever` sessions
- [ ] Local log files are written alongside DB inserts
- [ ] Control plane events appear in logs with source "control_plane"
- [ ] `cargo test -p server` passes
