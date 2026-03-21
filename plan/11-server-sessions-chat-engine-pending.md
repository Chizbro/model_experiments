# 11 — Server: sessions API and chat workflow engine

**Status:** pending  
**Dependencies:** 07, 10

## Objective

**Session lifecycle** for **chat** first: create session, enqueue jobs, **`POST /sessions/:id/input`**, worker **`task_complete`**, session status transitions. Aligns with Phase 1 in [PROJECT_KICKOFF §5](../docs/PROJECT_KICKOFF.md#5-phases--milestones-approved).

## Scope

**In scope**

- `POST /sessions`, `GET /sessions`, `GET /sessions/:id`, `PATCH /sessions/:id` (`retain_forever`), `PATCH /sessions/:id/jobs/:job_id` (`retain_forever`), `POST /sessions/:id/input`.
- Initial **chat** job creation; follow-up jobs on input; reject input with **409** when invalid.
- Merge **identity** + session param tokens into **pull payload** `credentials` ([API_OVERVIEW — Pull task](../docs/API_OVERVIEW.md#pull-task)).
- **Get session** returns `jobs[]` with `error_message`, `pull_request_url` fields (null ok).

**Out of scope**

- History cap / `history_truncated` (task 12).
- Loop workflows (task 13).

## Spec references

- [API_OVERVIEW §4](../docs/API_OVERVIEW.md#4-rest--sessions)
- [ARCHITECTURE §4 — Task flow](../docs/ARCHITECTURE.md#4-task--workflow-execution-flow)

## Acceptance criteria

- End-to-end **server-only** test: create session → pull assigns job → complete → session updated (optionally use test worker HTTP stub).

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test` integration | CI |
