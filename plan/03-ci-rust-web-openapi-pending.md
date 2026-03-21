# 03 — CI: Rust, Web, OpenAPI drift

**Status:** pending  
**Dependencies:** 01, 02 (OpenAPI file exists); Web scaffold can be minimal until 22

## Objective

**Single green pipeline** covering [CICD_DESIGN §2](../docs/CICD_DESIGN.md#2-jobs-design): fmt, clippy, test, web build/lint, and **failure on contract drift** once OpenAPI is authoritative.

## Scope

**In scope**

- Rust: `cargo fmt --check`, `clippy -D warnings`, `cargo test`, `cargo build --all-targets`.
- Web (when `package.json` exists): install, lint, typecheck, build.
- OpenAPI: step that **fails** if generated types or a maintained snapshot diverges (exact mechanism chosen in 00/02—e.g. `openapi-diff`, regen check, or “rust embed matches file”).

**Out of scope**

- Choosing Git host—use a placeholder workflow path (e.g. `.github/workflows/ci.yml`) and document in [CICD_DESIGN §4](../docs/CICD_DESIGN.md#4-not-yet-decided) when finalized.

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
