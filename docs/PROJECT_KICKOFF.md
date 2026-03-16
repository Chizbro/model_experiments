# Project Kickoff Checklist

Use this as a living checklist for the start of the project. Tick items as you complete or decide them.

---

## 1. Definition & Scope

- [x] **Product vision** documented ([PRODUCT.md](PRODUCT.md))
- [x] **Feature list** with priorities ([PRODUCT.md](PRODUCT.md))
- [x] **Architecture and communications** described ([ARCHITECTURE.md](ARCHITECTURE.md))
- [x] **Tech stack** chosen with rationale ([TECH_STACK.md](TECH_STACK.md))
- [x] **Out-of-scope** list agreed (see PRODUCT.md; final as-is per [Decisions](DECISIONS.md) §11)
- [x] **Success criteria** for first milestone agreed (see PRODUCT.md; approved as-is per [Decisions](DECISIONS.md) §12)

---

## 2. Repo & Tooling

- [ ] **Version control** initialized (`git init`), default branch set
- [ ] **Monorepo layout** created (e.g. `crates/server`, `crates/worker`, `crates/cli`, `crates/api-types`, `web/`, `docs/`)
- [ ] **Language/framework** choice locked (Rust for control plane, workers, and CLI)
- [ ] **Linting/formatting** configured (e.g. cargo fmt/clippy for Rust crates, ESLint/Prettier for web)
- [ ] **CI**: design in [CICD_DESIGN.md](CICD_DESIGN.md); implement when CI platform and git host are chosen (see [Decisions](DECISIONS.md) §8b, §8c)

---

## 3. Environment & Dependencies

- [ ] **Control plane** runnable locally (config for DB, port; task queue in Postgres)
- [ ] **Worker** runnable with env/config pointing at control plane URL
- [ ] **CLI** build/install and config (e.g. `CONTROL_PLANE_URL`, `API_KEY`)
- [ ] **Web UI** dev server and proxy to API (or CORS) for local dev
- [ ] **Secrets** approach: env vars vs secret store (document in TECH_STACK or README)

---

## 4. Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Worker and control plane version skew | Incompatible protocols or payloads | Version in API/register; server rejects too-old workers; document min versions. |
| Log volume | Storage and cost | Retention policy; sampling or log level in config; consider log backend (e.g. Loki) early. |
| Long-running sessions (inbox agents) | Memory/state growth | Cap inbox size; evict old completed jobs; optional checkpointing. |
| Git credentials on workers | Security | Env or per-job token; avoid storing on disk in plain text. |
| Single control plane failure | No dispatch or visibility | Run control plane with persistence (DB); optional HA later (multi-node + shared DB). |

Add project-specific risks as you go.

---

## 5. Phases / Milestones (approved)

| Phase | Goal | Deliverables |
|-------|------|--------------|
| **0 – Setup** | Repo, stack, local run | Cargo workspace, `crates/server`, `crates/worker`, `crates/cli`, `crates/api-types`; server + worker + CLI skeletons; DB schema (workers, sessions, jobs). |
| **1 – Minimal loop** | One task end-to-end | **Chat** workflow first (see [Decisions §21](DECISIONS.md#21-phase-1-first-workflow)); one worker; clone, run, commit to main; logs to control plane. |
| **2 – CLI & UI** | User-facing control | CLI: start session, list, tail logs. Web: same + attach. Session attach from both. |
| **3 – Loops & discovery** | Loops and multi-worker | Loop N and loop-until-sentinel; second worker; auto-registration. |
| **4 – Inboxes** | Continuous agents | Inbox per agent; continuous worker; spawn task to another agent’s inbox. |
| **5 – Polish** | Log search, PR/MR, secrets | Retention, search, PR/MR creation, secrets management (as in feature list). |

Adjust order and scope to match your priorities.

---

## 6. Communication & Docs

- [ ] **README** at repo root: what this is, how to run server/worker/CLI/UI, link to docs
- [ ] **Docs** in `docs/`: ARCHITECTURE, TECH_STACK, PRODUCT, this kickoff
- [ ] **API**: OpenAPI (or equivalent) spec from the start; keep in sync with server. Document WebSocket events in spec or companion doc. See [API_OVERVIEW.md](API_OVERVIEW.md) until spec exists.
- [ ] **Changelog**: deferred until formal releases (see [Decisions](DECISIONS.md) §10)

---

## 7. Next Actions

1. **Create Cargo workspace** and crate layout (`server`, `worker`, `cli`, `api-types`).
2. **Implement Phase 0**: get control plane and one worker talking (register, one task, logs).
3. **Define API contracts**: REST endpoints and WebSocket message shapes for sessions, logs, attach.
4. **Implement first workflow** (chat) with Git commit.
5. **Add CLI and Web UI** for start session + tail logs + attach.

---

*Back to [README](../README.md) | [Architecture](ARCHITECTURE.md) | [Tech Stack](TECH_STACK.md) | [Product](PRODUCT.md)*
