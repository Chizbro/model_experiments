# 20 — CLI: foundation, config precedence, human errors

**Status:** pending  
**Dependencies:** 02, 06

## Objective

`remote-harness` **clap** binary with config load order, API client wrapper, and **stderr** errors: HTTP status + `error.code` + `error.message` ([CLIENT_EXPERIENCE §2.2](../docs/CLIENT_EXPERIENCE.md#22-cli-mapping), [PROJECT_KICKOFF §6a — I](../docs/PROJECT_KICKOFF.md#6a-implementation-plan-concrete-checkpoints)—**no `--json` in v1**).

## Scope

**In scope**

- `config show`; env + `~/.config/remote-harness/config.yaml` precedence ([TECH_STACK §3](../docs/TECH_STACK.md#3-cli--rust)).
- Shared HTTP + SSE client utilities (SSE used in 21).

**Out of scope**

- Full command surface (task 21).

## Spec references

- [API_OVERVIEW — Spec delivery](../docs/API_OVERVIEW.md#spec-delivery-implementation-requirement)

## Acceptance criteria

- Tests: missing key → non-zero exit + stderr shape; config precedence unit tests.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test -p cli` | CI |
