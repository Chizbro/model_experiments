# Documentation index

Quick map of `docs/`—each file has one primary job. Use this to avoid reading the same topic in three places by accident.

| Doc | Job |
|-----|-----|
| [PRODUCT.md](PRODUCT.md) | What it is, priorities, success criteria, BYOL, glossary |
| [ARCHITECTURE.md](ARCHITECTURE.md) | How components fit together, worker reclaim, logging, Git §9a/9b, platform workers |
| [TECH_STACK.md](TECH_STACK.md) | Technology choices and rationale (Rust, Postgres, SPA, auth layout) |
| [API_OVERVIEW.md](API_OVERVIEW.md) | REST/SSE contracts (+ spec delivery: OpenAPI sync, CLI/Web alignment); pair with [`crates/server/openapi.yaml`](../crates/server/openapi.yaml) |
| [SSE_EVENTS.md](SSE_EVENTS.md) | SSE event shapes (companion to OpenAPI for `text/event-stream` payloads) |
| [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md) | Required UI/CLI behavior (errors, SSE, credentials, outcomes) |
| [HOSTING.md](HOSTING.md) | Topologies, Docker/restart/sleep, CORS, production checklist, threat model |
| [TROUBLESHOOTING.md](TROUBLESHOOTING.md) | Symptom → fix (operators); points to canonical sections above |
| [GETTING_STARTED.md](GETTING_STARTED.md) | Local dev: **Docker Compose** (§1, recommended) or **bare metal** (§2); shared credentials/OAuth (§3) |
| [GIT_CLONE_SPEC.md](GIT_CLONE_SPEC.md) | Worker/Git HTTPS + libgit2 requirements (implementation) |
| [CICD_DESIGN.md](CICD_DESIGN.md) | What CI should run (Rust, Web, OpenAPI drift) |
| [IMPLEMENTATION_BOUNDARIES.md](IMPLEMENTATION_BOUNDARIES.md) | Crate graph, OpenAPI vs `api-types`, config precedence, SSE pattern, test pyramid commands |
| [PROJECT_KICKOFF.md](PROJECT_KICKOFF.md) | Checklists, phases, **§6a implementation checkpoints** |
| [PHASE2_DESIGN.md](PHASE2_DESIGN.md) | **P1 design:** personas resolution, inboxes, PR/MR matrix, log search (before large Phase 2 PRs) |
| [plan/README.md](../plan/README.md) | **Implementation plan:** ordered, testable tasks (`plan/*-pending.md`) |

### Suggested paths

- **New developer:** PRODUCT → [GETTING_STARTED.md](GETTING_STARTED.md) (§1 Docker) → [IMPLEMENTATION_BOUNDARIES.md](IMPLEMENTATION_BOUNDARIES.md) (where code lives) → ARCHITECTURE skim → API_OVERVIEW
- **UI/CLI implementer:** API_OVERVIEW (Spec delivery) + CLIENT_EXPERIENCE + PRODUCT (features)
- **Operator / deploy:** HOSTING §13–14 → TROUBLESHOOTING → CLIENT_EXPERIENCE (expectations)
- **Worker / Git bugs:** GIT_CLONE_SPEC → ARCHITECTURE §9a–9b → TROUBLESHOOTING §2b

### Known overlap (by design)

- **Docker + sleep + restart:** HOSTING is canonical depth; [GETTING_STARTED §1](GETTING_STARTED.md#1-docker-compose-recommended) and ARCHITECTURE only summarize—add detail in HOSTING only.

### Repository layout

These specs describe a **Rust monorepo** with `crates/server`, `crates/worker`, `crates/cli`, `crates/api-types`, `web/`, and (typically) root **`docker-compose.yml`** / **`Dockerfile`**, as in [TECH_STACK §7](TECH_STACK.md#7-repo-layout-rust-monorepo) and [GETTING_STARTED](GETTING_STARTED.md). **This repository** includes that layout at the repo root (see [plan task 01](../plan/01-monorepo-docker-tooling-complete.md)). **Docs-only checkouts** of the same specifications should run **GETTING_STARTED** in the repo that holds the workspace; keep that repo’s tree aligned with the docs so paths (e.g. `crates/worker/src/git_ops.rs` in [GIT_CLONE_SPEC](GIT_CLONE_SPEC.md)) resolve.
