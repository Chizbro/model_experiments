# 25 — Web: Git outcomes, retention copy, long-chat banner

**Status:** complete  
**Dependencies:** 23, 24

## Objective

Close **PROJECT_KICKOFF §6a** UX gaps that are **Web-primary** (CLI gets parallel copy in 21 where applicable): [E — outcomes](../docs/PROJECT_KICKOFF.md#6a-implementation-plan-concrete-checkpoints), [G — retention surfacing](../docs/PROJECT_KICKOFF.md#6a-implementation-plan-concrete-checkpoints), [C — history truncation copy](../docs/PROJECT_KICKOFF.md#6a-implementation-plan-concrete-checkpoints).

## Scope

**In scope**

- Job/session UI: **`error_message`** prominent; **no silent “missing MR”** when `branch_mode === pr` and `pull_request_url` null—explain using [CLIENT_EXPERIENCE §8](../docs/CLIENT_EXPERIENCE.md#8-git-commit-push-and-prmr-outcomes) + [Architecture §9b](../docs/ARCHITECTURE.md#9b-when-the-control-plane-creates-a-prmr).
- **Settings** (or About): default retention period, `retain_forever` discovery ([PRODUCT L5](../docs/PRODUCT.md#logging--observability)).
- When `task_input.history_truncated` surfaces via session/detail API or job metadata, show [CLIENT_EXPERIENCE §12](../docs/CLIENT_EXPERIENCE.md#12-long-chat-sessions) copy (if flag only on pull payload, document what UI uses—server may expose “truncation occurred” on session for UX).

**Out of scope**

- Changing API without updating [API_OVERVIEW.md](../docs/API_OVERVIEW.md) and OpenAPI.

## Spec references

- [CLIENT_EXPERIENCE §8–9, §12](../docs/CLIENT_EXPERIENCE.md)
- [PRODUCT I4, L5](../docs/PRODUCT.md)

## §6a checklist (E / G / C) — implementation mapping

| Checkpoint | Where it shows up |
|------------|-------------------|
| **E** — `error_message` prominent | Session detail **alert** listing every job with a non-empty `error_message`; jobs table **row tint** + **bold** error text in the Error column. |
| **E** — no silent missing MR / Git outcomes | `sessionJobOutcomeNotes`: `pullRequestExpectationHint`, `commitPushOutcomeHint` (no `commit_ref` when Git expected), `failedJobRemoteCommitHint` (failed + `commit_ref`); **Commit** column shows truncated `commit_ref`. |
| **G** — retention + retain_forever | **Settings → Data & log retention**: `log_retention_days_default` and `chat_history_max_turns` from **`GET /health`**; copy points to session detail **retain forever** (existing checkbox). |
| **C** — long chat copy | Server **`GET /sessions/:id`**: `chat_history_truncated`, `chat_history_max_turns` (chat workflow; aligned with pull cap). Session detail **amber banner** with CLIENT_EXPERIENCE §12 wording. Documented in **API_OVERVIEW** + OpenAPI. |

## Acceptance criteria

- [x] Review checklist mapping each §6a E/G/C bullet to a visible UI element or documented deferral.
- [x] Snapshot or Storybook for error states — **Vitest inline snapshots** in `web/src/lib/sessionJobHints.test.ts` (PR / failed+commit representative strings).

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | Visual/component tests | Manual QA pass before release |

**Automated:** `cargo test`, `cargo clippy --all-targets -- -D warnings`, `web/npm test`, `web/npm run build`.
