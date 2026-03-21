# 00 — Upfront design: crate boundaries and test strategy

**Status:** complete  
**Dependencies:** none (do this before or alongside 01)

## Objective

Lock in **clean architecture** before cargo/workspace growth: where code lives, how config flows, and how we test so later tasks do not accrete “spaghetti” shortcuts.

## Deliverables

1. Short **implementation companion** (e.g. `docs/IMPLEMENTATION_BOUNDARIES.md` or a section in an existing doc—**must be linked from** [docs/README.md](../docs/README.md)) covering:
   - **Crate graph:** `server`, `worker`, `cli`, `api-types` (and optional `server` submodules for `engine`, `api`, `db`).
   - **Single source of truth** for HTTP contracts: OpenAPI path (see task 02) and who may duplicate types (prefer `api-types` + codegen or hand-written mirrors with tests).
   - **Config:** env vars and file precedence for server, worker, CLI (align with [TECH_STACK §3](../docs/TECH_STACK.md#3-cli--rust), worker env in same doc family).
   - **SSE vs REST:** one place documenting subscriber/broadcast pattern for log and session streams (avoid ad-hoc channels per handler).
2. **Test pyramid** decision for this repo: unit (pure logic), `cargo test` integration with test Postgres (or testcontainers), Web `vitest`/optional Playwright—**named** so task owners use the same commands.

## Spec references

- [ARCHITECTURE §2a — Migrations](../docs/ARCHITECTURE.md#2a-schema-migrations)
- [TECH_STACK §7 — Repo layout](../docs/TECH_STACK.md#7-repo-layout-rust-monorepo)
- [API_OVERVIEW — Spec delivery](../docs/API_OVERVIEW.md#spec-delivery-implementation-requirement)

## Acceptance criteria

- Document is merged and discoverable from `docs/README.md`.
- No implementation code required; this is design-only. If you choose to add only a stub `IMPLEMENTATION_BOUNDARIES.md` in task 01, **complete the content in this task first** or merge 00+01 in order.

## Testing

| When | What | Retest |
|------|------|--------|
| After writing | Peer review + link check from docs index | N/A |

---

## Completed / Notes

- Added [docs/IMPLEMENTATION_BOUNDARIES.md](../docs/IMPLEMENTATION_BOUNDARIES.md) (crate graph, HTTP contract SSoT, config precedence, SSE subscriber pattern, test pyramid commands).
- Linked from [docs/README.md](../docs/README.md) and [plan/README.md](README.md) canonical table.
- This checkout is **docs-only** (no `Cargo.toml` / `web/`); no `cargo`/`npm` build was runnable here.
