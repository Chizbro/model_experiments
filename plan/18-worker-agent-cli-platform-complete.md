# 18 — Worker: Claude Code / Cursor invocation (platform-specific)

**Status:** complete  
**Dependencies:** 16

## Objective

**Isolate OS-specific** process spawning: Windows native vs WSL vs macOS vs Linux for **argument quoting**, stdin vs argv, stdout/stderr streaming, optional PTY needs ([ARCHITECTURE §4c](../docs/ARCHITECTURE.md#4c-platform-specific-workers-cli-invocation), [TECH_STACK §2](../docs/TECH_STACK.md#2-workers-rust)).

## Scope

**In scope**

- Module per platform or `cfg(target_os)` matrix with shared trait `AgentCliRunner`.
- Map `agent_cli` enum from task payload to binary + args; inject `agent_token` per vendor docs **without** logging it.
- Stream child output to **worker logs** + accumulate **assistant_reply** / **output** snippet for `task_complete` ([API_OVERVIEW — Task complete](../docs/API_OVERVIEW.md#task-complete)).

**Out of scope**

- Installing CLIs—document prerequisites.

## Spec references

- [PRODUCT — BYOL](../docs/PRODUCT.md#bring-your-own-licence-byol)
- [ARCHITECTURE §9a](../docs/ARCHITECTURE.md#9a-when-the-worker-attempts-commit-and-push)

## Acceptance criteria

- **Unit tests** with **fake child** or recorded subprocess interface (no real vendor in CI).
- Manual smoke checklist documented for one OS (developer runbook).

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test -p worker` platform-agnostic paths | CI + manual per platform |

## Completed / Notes

- Added `crates/worker/src/agent_cli/`: `AgentCliKind`, `WorkerPlatform` + WSL detection, `register_platform_label` for worker register, `build_invocation` / `run_invocation`, `TracingAgentLogSink`, `CapturedAgentOutput`, `UnixAgentCliRunner` / `WindowsAgentCliRunner` (`CREATE_NO_WINDOW`).
- Docs: [ARCHITECTURE.md §4c](../docs/ARCHITECTURE.md#4c-platform-specific-workers-cli-invocation) implementation pointer; [worker README](../crates/worker/README.md) env table + macOS manual smoke; [IMPLEMENTATION_BOUNDARIES.md](../docs/IMPLEMENTATION_BOUNDARIES.md) worker row.
- End-to-end execution is wired in plan task **19** (`task_execution`, logs + complete).
