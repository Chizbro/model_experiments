# 25 — Web: Git outcomes, retention copy, long-chat banner

**Status:** pending  
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

## Acceptance criteria

- Review checklist mapping each §6a E/G/C bullet to a visible UI element or documented deferral.
- Snapshot or Storybook for error states.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | Visual/component tests | Manual QA pass before release |
