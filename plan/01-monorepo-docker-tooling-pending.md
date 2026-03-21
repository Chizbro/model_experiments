# 01 — Monorepo layout, Rust workspace, Docker

**Status:** pending  
**Dependencies:** 00 (recommended)

## Objective

Establish the **Rust workspace**, crate skeletons (`server`, `worker`, `cli`, `api-types`), `web/` placeholder or scaffold, root **`Dockerfile`** and **`docker-compose.yml`** per [GETTING_STARTED](../docs/GETTING_STARTED.md) and [TECH_STACK §7](../docs/TECH_STACK.md#7-repo-layout-rust-monorepo).

## Scope

**In scope**

- `Cargo.toml` workspace; bin/lib stubs that compile.
- Compose services: `postgres`, `server` (depends on healthy DB), optional `worker` stub or comment for later.
- `rust-toolchain.toml` or documented toolchain pin.
- Root **README** stub pointing at docs (full README can expand in 26).

**Out of scope**

- Real HTTP handlers (task 04+).
- Production hardening beyond dev defaults.

## Spec references

- [PROJECT_KICKOFF §2](../docs/PROJECT_KICKOFF.md#2-repo--tooling)
- [HOSTING](../docs/HOSTING.md) (Compose patterns summarized in GETTING_STARTED)

## Acceptance criteria

- `cargo build --workspace` succeeds.
- `docker compose config` validates; `docker compose up` can start Postgres (and server container builds, even if server only exposes health stubs later).

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo build --workspace`; `docker compose config` | CI job from task 03 |
