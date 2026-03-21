# 13 — Server: `loop_n` and `loop_until_sentinel` (literal only)

**Status:** pending  
**Dependencies:** 11, 10

## Objective

**Loop workflows** with **one job per iteration** ([ARCHITECTURE §4](../docs/ARCHITECTURE.md#4-task--workflow-execution-flow)): `loop_n` creates N jobs upfront; `loop_until_sentinel` creates jobs dynamically until worker reports sentinel matched ([PRODUCT W3](../docs/PRODUCT.md#workflows), [PROJECT_KICKOFF §6a — B](../docs/PROJECT_KICKOFF.md#6a-implementation-plan-concrete-checkpoints)).

## Scope

**In scope**

- `POST /sessions` accepts `loop_n` and `loop_until_sentinel` with params per API_OVERVIEW.
- **v1 sentinel:** literal substring only—**no regex**; case sensitivity **implementation-defined**—choose **case-sensitive default**, document in server config/README ([API_OVERVIEW create session](../docs/API_OVERVIEW.md#create-session-start-workflow)).
- Worker signals “sentinel found” via `task_complete` payload (or dedicated field)—**document in OpenAPI**; server stops enqueuing further iterations.

**Out of scope**

- Regex `sentinel_mode` (future P1).

## Spec references

- [API_OVERVIEW §4](../docs/API_OVERVIEW.md#4-rest--sessions)
- [PRODUCT — Glossary Sentinel](../docs/PRODUCT.md#glossary)

## Acceptance criteria

- Tests: `loop_n` exhausts exactly N jobs; sentinel stops after k iterations; mismatch continues until max guard if you add iteration cap—**define** max for unbounded sentinel (config) to avoid infinite sessions.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test` engine scenarios | CI |
