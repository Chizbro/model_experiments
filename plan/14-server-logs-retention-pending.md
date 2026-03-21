# 14 — Server: log ingest, pagination, delete, retention purge

**Status:** pending  
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
