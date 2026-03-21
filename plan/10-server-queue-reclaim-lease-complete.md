# 10 — Server: task queue, pull_task, reclaim, lease

**Status:** complete  
**Dependencies:** 05, 06, 09

## Objective

Transactional **`POST /workers/tasks/pull`** that: reclaims jobs from **stale** workers, optionally **lease-expires** long-running assigned jobs, respects **`max_job_reclaims`**, then assigns the next **pending** job to the pulling worker ([ARCHITECTURE §3b](../docs/ARCHITECTURE.md#3b-worker-death-job-reclaim-and-bounded-retries)).

## Scope

**In scope**

- **SQL** reclaim step as in architecture doc (single transaction before select pending).
- `reclaim_count` increment; fail with `[MAX_WORKER_LOSS_RETRIES]` message when over cap.
- `job_lease_seconds`: if > 0, fail stuck `assigned` jobs with `[JOB_LEASE_EXPIRED]` (phase 2 in architecture—implement if spec treats as required for shippable reclaim story; default 0 off).
- **Idempotent** pull: worker gets consistent `job_id` assignment; **204** or empty when no work.
- `task_complete` / failure endpoint from worker ([API_OVERVIEW §9](../docs/API_OVERVIEW.md#9-worker--control-plane))—implement in same task or tightly coupled follow-up in 11 if you split; **do not** leave jobs stuck without complete path.

**Out of scope**

- Building full `task_input` payload for all workflows (11–13).

## Spec references

- [ARCHITECTURE §3b](../docs/ARCHITECTURE.md#3b-worker-death-job-reclaim-and-bounded-retries)
- [CLIENT_EXPERIENCE §6](../docs/CLIENT_EXPERIENCE.md#6-jobs-failures-outside-the-users-control)

## Acceptance criteria

- Integration tests: stale worker → job returns pending; reclaim cap → failed job message; lease → expired path (if enabled).
- All behavior covered by tests to avoid subtle transaction bugs.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test` with real Postgres transactions | CI |
