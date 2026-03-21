# 09 — Server: worker registry, heartbeat, version gate

**Status:** pending  
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
- CLI/Web strings for this error deferred to 21/25 but **server contract** is complete.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test` | CI |
