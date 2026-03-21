# 20 — CLI: foundation, config precedence, human errors

**Status:** complete  
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

---

## Completed / Notes

- Added `crates/cli` library (`lib.rs`): `config_file`, `resolved` (precedence + unit tests), `http_util::format_http_api_error`, `sse::SseReader` for task 21.
- Global flags no longer use clap `env =` so precedence is strictly **CLI → env (`REMOTE_HARNESS_URL` / `CONTROL_PLANE_URL`, `REMOTE_HARNESS_API_KEY` / `API_KEY`) → YAML file → default URL**.
- `remote-harness config show` prints path, resolved URL/API key (masked), and winning source.
- Integration test `crates/cli/tests/missing_api_key.rs` for missing API key on `session list`.
