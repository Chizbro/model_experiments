# 19 — Worker: end-to-end task execution

**Status:** pending  
**Dependencies:** 16, 17, 18, 11–14 (server)

## Objective

Full **pull → clone → run agent → commit/push (per params) → POST logs → POST complete** loop, including **chat** `assistant_reply`, **loop_until_sentinel** `output` + `sentinel_reached`, and Git failure messages surfaced via `error_message` ([ARCHITECTURE §9a–9b](../docs/ARCHITECTURE.md#9a-when-the-worker-attempts-commit-and-push)).

## Scope

**In scope**

- Work directory lifecycle per job (clean clone or reuse policy—document).
- **`branch_mode`**: main vs PR branch behavior per spec; partial PR creation may be stub if P1 ([PRODUCT O2](../docs/PRODUCT.md#optional--later))—but **must not** silently claim MR exists ([CLIENT_EXPERIENCE §8](../docs/CLIENT_EXPERIENCE.md#8-git-commit-push-and-prmr-outcomes)).
- **Planning step** failures (branch/MR text) as user-visible job errors where Architecture distinguishes them.

**Out of scope**

- Worker-side WebSocket/SSE (logs are POST batches only).

## Spec references

- [API_OVERVIEW — Send logs, Task complete](../docs/API_OVERVIEW.md#send-logs)
- [GIT_CLONE_SPEC](../docs/GIT_CLONE_SPEC.md)

## Acceptance criteria

- **Integration**: with test server + stub agent binary or `echo` shim, one full job succeeds; one failure path sets `error_message`.
- Compose smoke (task 26) runs **chat** on a real repo in dev.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test -p worker -- --ignored` optional E2E; primary integration in CI | CI + manual dev |
