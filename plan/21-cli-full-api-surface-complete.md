# 21 — CLI: sessions, workers, logs, credentials, API keys

**Status:** complete  
**Dependencies:** 20, server tasks for corresponding endpoints

## Objective

Implement **TECH_STACK command map** ([TECH_STACK §3](../docs/TECH_STACK.md#3-cli--rust)) for every **shipped** server capability—**same release** as server/Web per [AGENTS.md](../AGENTS.md).

## Scope

**In scope**

- Sessions: start, list, show, delete if API exists; **attach** (SSE session events + optional log follow); **input** for chat.
- Workers: list, clear.
- Logs: **tail** = paginate full history then SSE ([API_OVERVIEW §6 client contract](../docs/API_OVERVIEW.md#6-rest--logs)); **delete**.
- Credentials: show/set status via identity endpoints; **api-key** create/list/revoke; **bootstrap** helper that calls `POST /api-keys/bootstrap` with operator warnings.
- **Inbox** CLI only when server inbox is implemented (else defer to 27).

**Out of scope**

- `--json` (explicitly v2+ unless spec amended).

## Spec references

- [API_OVERVIEW summary table](../docs/API_OVERVIEW.md#10-summary)
- [CLIENT_EXPERIENCE §13](../docs/CLIENT_EXPERIENCE.md#13-compatibility-and-upgrades) — map `worker_version_incompatible` when hitting server from worker troubleshooting helpers if any

## Acceptance criteria

- **Integration tests** against test server for main commands (or scripted `assert_cmd`).
- Help text links to `docs/` not duplicate normative API prose.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test -p cli` + manual spot checks | CI |

---

## Completed / Notes

- **CLI:** Root `--help` `long_about` points at `docs/API_OVERVIEW.md`, `docs/SSE_EVENTS.md`, `docs/TECH_STACK.md`, `docs/GETTING_STARTED.md`, `docs/CLIENT_EXPERIENCE.md`. Added `session start` (alias of `create`), `session delete`, `logs tail` (full history or `--last` then `GET .../logs/stream` via **ureq** streaming `Read` + existing `SseReader`), top-level `attach` (`GET .../events`, optional `--follow-logs` for parallel log SSE thread), `credentials show` / `credentials set` (wraps identity GET + auth-status / PATCH), `workers` visible alias for `worker`, `clear` alias for `worker delete`, `apikey` alias for `api-key`, `revoke` alias for `api-key delete`.
- **Library:** `crates/cli/src/log_ops.rs` — paginated log history + SSE open helpers.
- **Server:** Implemented documented but missing **`DELETE /sessions/:id`** (OpenAPI `deleteSession`, integration test `delete_session_returns_204_and_get_is_404` when `DATABASE_URL` set).
- **Web:** `DELETE /sessions/:id` control next to session detail (confirm dialog) per AGENTS.md.
- **Tests:** `crates/cli/tests/help_and_subcommands.rs` (`assert_cmd` + `predicates`).
