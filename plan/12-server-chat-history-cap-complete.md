# 12 — Server: chat history cap and `history_truncated`

**Status:** complete  
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

## Completed / Notes

- `crates/server/src/config.rs`: `chat_history_max_turns` from env `CHAT_HISTORY_MAX_TURNS` (default `50`; `0` = disable capping).
- `crates/server/src/worker_tasks.rs`: `apply_chat_history_cap_on_pull` trims `history` and `history_assistant` to the last *N* items each for chat follow-up payloads (objects with `session_prompt`); sets `history_truncated` on every follow-up pull response. Wired through `build_task_input` → `finish_pull_response`.
- Unit tests: `worker_tasks::chat_history_cap_tests`. Integration: `sessions_integration::chat_pull_truncates_history_per_config_on_pull` (`DATABASE_URL`, cap `2`).
- Docs/plan index updates: `docs/GETTING_STARTED.md`, `docs/PROJECT_KICKOFF.md`, `plan/README.md`, `plan/11-server-sessions-chat-engine-complete.md`.
