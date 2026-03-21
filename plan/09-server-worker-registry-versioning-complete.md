# 09 — Server: worker registry, heartbeat, version gate

**Status:** complete  
**Dependencies:** 06

## Objective

Workers **register** with **`client_version` required** for v1 implementations; incompatible versions get **`400`** + `error.code: "worker_version_incompatible"` ([API_OVERVIEW §9 — Register](../docs/API_OVERVIEW.md#register), [CLIENT_EXPERIENCE §13](../docs/CLIENT_EXPERIENCE.md#13-compatibility-and-upgrades), [PROJECT_KICKOFF §6a — A](../docs/PROJECT_KICKOFF.md#6a-implementation-plan-concrete-checkpoints)).

## Scope

**In scope**

- `POST /workers/register`, `POST /workers/:id/heartbeat`, `GET /workers`, `GET /workers/:id`, `DELETE /workers/:id`.
- **Stale** worker computation from `last_seen_at` + server config (`worker_stale_seconds`).
- **Version policy:** document semver rule (e.g. same major.minor as server); enforce in register.
- On `DELETE /workers/:id`, reclaim assigned jobs per [ARCHITECTURE §3b](../docs/ARCHITECTURE.md#3b-worker-death-job-reclaim-and-bounded-retries) (coordinate with 10 if ordering overlaps—implement delete behavior when queue exists).

**Out of scope**

- Pull task assignment (task 10).

## Spec references

- [API_OVERVIEW §5, §9](../docs/API_OVERVIEW.md)
- [ARCHITECTURE §3](../docs/ARCHITECTURE.md#3-worker-discovery--registration)

## Acceptance criteria

- Integration tests: compatible register; incompatible returns exact error code; heartbeat updates `last_seen_at`; list shows active vs stale.
- **Done:** CLI (`remote-harness worker …`) and Web UI Workers section expose the same operations per [AGENTS.md](../AGENTS.md).

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test` | CI |

---

## Completed / Notes

- **Migration** `20250320120000_workers_version_capabilities.sql`: `workers.client_version`, `workers.capabilities` (JSONB).
- **Config:** `MAX_JOB_RECLAIMS` (default `3`) on `ServerConfig`; used for `DELETE /workers/:id` reclaim vs fail path.
- **Version gate:** `semver` crate; worker `client_version` must match server `api_types::CRATE_VERSION` **major.minor**; else `worker_version_incompatible`. Missing `client_version`: accepted with `eprintln!` warning (transitional per API).
- **OpenAPI:** `registerWorker`, `heartbeatWorker`, `listWorkers`, `getWorker`, `deleteWorker` + schemas.
- **Tests:** `crates/server/tests/workers_integration.rs` (requires `DATABASE_URL`).
