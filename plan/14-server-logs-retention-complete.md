# 14 — Server: log ingest, pagination, delete, retention purge

**Status:** complete  
**Dependencies:** 11, 05

## Objective

**Central log store** in Postgres: workers **POST** log batches; clients **GET** paginated history; **DELETE** clears central store rows; **retention** job purges old logs except `retain_forever` ([API_OVERVIEW §6](../docs/API_OVERVIEW.md#6-rest--logs), [PRODUCT L5](../docs/PRODUCT.md#logging--observability), [CLIENT_EXPERIENCE §9](../docs/CLIENT_EXPERIENCE.md#9-log-retention-and-purge)).

## Scope

**In scope**

- Worker log ingest endpoint(s) from [API_OVERVIEW §9](../docs/API_OVERVIEW.md#9-worker--control-plane) (batch append).
- `GET /sessions/:id/logs` with cursor pagination + optional `job_id`, `level`, `last`.
- `DELETE /sessions/:id/logs` with optional `job_id`.
- Scheduled or on-tick purge: default **7 days** configurable; honor session/job `retain_forever`.

**Out of scope**

- SSE (task 15).
- Dual-write **files** on server may be stub—full [ARCHITECTURE §6](../docs/ARCHITECTURE.md#6-logging-architecture) can be phased but document what’s done.

## Spec references

- [ARCHITECTURE §6](../docs/ARCHITECTURE.md#6-logging-architecture)

## Acceptance criteria

- Integration tests: ingest → list → delete; purge removes old rows and preserves retained.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test` + optional time-mocked purge | CI |

## Completed / Notes

- Migration `20250320160000_logs_api_fields.sql`: `log_level`, `log_source`, `worker_id`, `occurred_at` on `logs`; index `(session_id, occurred_at, id)`.
- Handlers in `crates/server/src/logs.rs`; routes wired in `lib.rs`; `POST /workers/tasks/:id/logs` requires job `assigned`.
- `run_log_retention_purge` + background interval in `main.rs` (`LOG_PURGE_INTERVAL_SECS`, default 3600; minimum tick 60s).
- `LOG_RETENTION_DAYS_DEFAULT` default **7** (was 30).
- api-types: `LogEntry`, `WorkerLogIngestItem`, `WorkerLogsAcceptedResponse`, `Paginated<LogEntry>`.
- OpenAPI + `openapi_contract` allowlist updated.
- CLI `logs list|delete|send`; Web **Logs** section in `App.tsx`.
- Integration: `logs_ingest_list_delete_last_filter_and_retention_purge` in `sessions_integration.rs`.
- ARCHITECTURE §6 implementation note for v1 central store vs file mirror.
