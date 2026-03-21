# 02 — Shared API types and OpenAPI baseline

**Status:** pending  
**Dependencies:** 01

## Objective

Create the **contract artifact** path (e.g. `crates/server/openapi.yaml` or repo-root `openapi/`) and **`api-types`** (or generated bindings) so server, CLI, and worker share one vocabulary. Establish the rule: **markdown API_OVERVIEW and OpenAPI stay in sync** ([API_OVERVIEW — Spec delivery](../docs/API_OVERVIEW.md#spec-delivery-implementation-requirement)).

## Scope

**In scope**

- OpenAPI 3.x with **`operationId`** for each REST op you implement in early milestones; start with health + error schema + a minimal slice, expand as tasks land.
- Decision recorded (in OpenAPI README header or `docs/PROJECT_KICKOFF.md`): **SSE documented in OpenAPI descriptions** vs companion **`docs/SSE_EVENTS.md`**—pick one and link it.
- Rust types for shared enums/strings (`workflow`, error body, pagination cursor pattern) in `api-types`.

**Out of scope**

- Covering every endpoint before they exist—add paths incrementally but **never leave drift** once an endpoint ships.

## Spec references

- [API_OVERVIEW](../docs/API_OVERVIEW.md)
- [PROJECT_KICKOFF §6 — OpenAPI / SSE](../docs/PROJECT_KICKOFF.md#6-communication--docs)
- [CICD_DESIGN §4](../docs/CICD_DESIGN.md#4-not-yet-decided)

## Acceptance criteria

- Checked-in OpenAPI file is the single REST contract reference for codegen/review.
- `api-types` crate compiles; at least one round-trip test (e.g. deserialize sample error JSON from §2).

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test -p api-types` | CI schema diff / fmt (task 03) |
