# 12 — Server: chat history cap and `history_truncated`

**Status:** pending  
**Dependencies:** 11

## Objective

Bound **multi-turn chat** `task_input` per [API_OVERVIEW — Pull task](../docs/API_OVERVIEW.md#pull-task): default **50** user + **50** assistant turns (configurable); set **`history_truncated`: true** when drops occur ([PROJECT_KICKOFF §6a — C](../docs/PROJECT_KICKOFF.md#6a-implementation-plan-concrete-checkpoints), [CLIENT_EXPERIENCE §12](../docs/CLIENT_EXPERIENCE.md#12-long-chat-sessions)).

## Scope

**In scope**

- Server config key (e.g. `CHAT_HISTORY_MAX_TURNS`) documented.
- Pull payload construction only—workers must forward opaque `task_input`.

**Out of scope**

- UI copy (task 25).

## Spec references

- [API_OVERVIEW — task_input chat follow-up](../docs/API_OVERVIEW.md#pull-task)

## Acceptance criteria

- Unit/integration test: > N turns → payload capped and flag true; ≤ N → flag false.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test` | CI |
