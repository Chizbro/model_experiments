# 03 — CI: Rust, Web, OpenAPI drift

**Status:** complete  
**Dependencies:** 01, 02 (OpenAPI file exists); Web scaffold can be minimal until 22

## Objective

**Single green pipeline** covering [CICD_DESIGN §2](../docs/CICD_DESIGN.md#2-jobs-design): fmt, clippy, test, web build/lint, and **failure on contract drift** once OpenAPI is authoritative.

## Scope

**In scope**

- Rust: `cargo fmt --check`, `clippy -D warnings`, `cargo test`, `cargo build --all-targets`.
- Web (when `package.json` exists): install, lint, typecheck, build.
- OpenAPI: step that **fails** if generated types or a maintained snapshot diverges (exact mechanism chosen in 00/02—e.g. `openapi-diff`, regen check, or “rust embed matches file”).

**Out of scope**

- Choosing Git host—use a placeholder workflow path (e.g. `.github/workflows/ci.yml`) and document in [CICD_DESIGN §4](../docs/CICD_DESIGN.md#4-platform-placeholder--remaining-decisions) when finalized.

## Spec references

- [CICD_DESIGN](../docs/CICD_DESIGN.md)
- [PROJECT_KICKOFF §6a — J](../docs/PROJECT_KICKOFF.md#6a-implementation-plan-concrete-checkpoints)

## Acceptance criteria

- CI configuration exists and passes on a clean tree.
- Intentional OpenAPI edit without regen causes CI failure (prove with a local dry run or documented test).

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | Push or `act` / local script mirroring CI | On every PR |

## Completed / Notes

- **GitHub Actions:** [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) — jobs `rust` and `web` (Node 22, `npm ci`).
- **Local mirror:** [`scripts/ci-local.sh`](../scripts/ci-local.sh).
- **OpenAPI drift:** `crates/server/tests/openapi_contract.rs` — parses `openapi.yaml`, asserts OpenAPI 3.x, and compares collected `operationId`s to `EXPECTED_OPERATION_IDS`. Mismatch fails `cargo test`. Fixed YAML quoting on security scheme descriptions so `serde_yaml` accepts the file.
- **Web:** Minimal Vite + React + TypeScript app under `web/` so `lint` / `typecheck` / `build` / `test` (Vitest, no files yet) succeed in CI; full shell remains plan task 22.
