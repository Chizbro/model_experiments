# 05 — Database schema and SQLx migrations

**Status:** complete  
**Dependencies:** 01

## Objective

**Forward-only SQLx migrations** in `crates/server/migrations` covering registry, sessions, jobs, workers, logs, API keys, identities—so the engine and API tasks can assume tables exist. Migrations run **on server startup** ([ARCHITECTURE §2a](../docs/ARCHITECTURE.md#2a-schema-migrations)).

## Scope

**In scope**

- Tables sufficient for: workers (`last_seen_at`, labels JSON), sessions, jobs (`status`, `reclaim_count`, `assigned_at`, `error_message`, `worker_id`, ...), logs, api_keys (hashed), identities (tokens stored securely—never log), optional personas/inbox placeholders if you prefer separate migrations later.
- Seed **`default`** identity per [API_OVERVIEW §4a](../docs/API_OVERVIEW.md#4a-rest--identities-byol-credentials).
- `sqlx::migrate!()` in server main after DB connect.

**Out of scope**

- Editing applied migrations—**always add new files** ([AGENTS.md](../AGENTS.md)).

## Spec references

- [ARCHITECTURE §3b](../docs/ARCHITECTURE.md#3b-worker-death-job-reclaim-and-bounded-retries)
- [API_OVERVIEW — sessions/jobs shapes](../docs/API_OVERVIEW.md#get-session)

## Acceptance criteria

- Fresh DB: server starts and applies all migrations cleanly.
- CI or `cargo sqlx` prepare step documented if offline compile requires it.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | Test Postgres + `migrate run` in integration test | CI + new migrations only append |

## Completed / Notes

- Initial migration `20250320100000_initial_schema.sql`: `identities` (seed `default`), `api_keys`, `workers`, `sessions` (`git_ref` column; `ref` is reserved in SQL), `jobs`, `logs`.
- `run_database_migrations` in `crates/server/src/lib.rs`; `crates/server/src/main.rs` runs migrations after `connect_database` when `DATABASE_URL` is set.
- SQLx features: `macros` + `migrate` (required for `sqlx::migrate!`).
- CI: `.github/workflows/ci.yml` runs Postgres 16 service and sets `DATABASE_URL` so `crates/server/tests/migrations_integration.rs` exercises migrations.
- **Offline compile:** `sqlx::migrate!` embeds migration SQL at compile time; `cargo sqlx prepare` / `.sqlx` data are for `query!` macros, not migrations ([GETTING_STARTED.md](../docs/GETTING_STARTED.md)).
