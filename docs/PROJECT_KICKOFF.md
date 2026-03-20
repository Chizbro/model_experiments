# Project Kickoff Checklist

Use this as a living checklist for the start of the project. Tick items as you complete or decide them.

**Specs vs implementation repo:** Architecture and API docs may describe behavior **before** every §2–§6 checkbox is done (e.g. job lease reclaim in [ARCHITECTURE §3b](ARCHITECTURE.md#3b-worker-death-job-reclaim-and-bounded-retries)). Treat §2–§3 as **mandatory for a shippable monorepo**, not as proof that prose is wrong if unchecked. Docs-only checkouts: see [docs/README.md — Repository layout](README.md#repository-layout).

---

## 1. Definition & Scope

- [x] **Product vision** documented ([PRODUCT.md](PRODUCT.md))
- [x] **Feature list** with priorities ([PRODUCT.md](PRODUCT.md))
- [x] **Architecture and communications** described ([ARCHITECTURE.md](ARCHITECTURE.md))
- [x] **Tech stack** chosen with rationale ([TECH_STACK.md](TECH_STACK.md))
- [x] **Out-of-scope** list agreed (see PRODUCT.md)
- [x] **Success criteria** for first milestone agreed (see PRODUCT.md)

---

## 2. Repo & Tooling

- [ ] **Version control** initialized (`git init`), default branch set
- [ ] **Monorepo layout** created (e.g. `crates/server`, `crates/worker`, `crates/cli`, `crates/api-types`, `web/`, `docs/`)
- [ ] **Language/framework** choice locked (Rust for control plane, workers, and CLI)
- [ ] **Build and run**: use **Docker** (see repo root `Dockerfile` and `docker-compose.yml`) for building and running the control plane and services; see [GETTING_STARTED.md §1](GETTING_STARTED.md#1-docker-compose-recommended).

---

## 3. Environment & Dependencies

- [ ] **Control plane** runnable locally (config for DB, port; task queue in Postgres)
- [ ] **Worker** runnable with env/config pointing at control plane URL
- [ ] **CLI** build/install and config (e.g. `CONTROL_PLANE_URL`, `API_KEY`)
- [ ] **Web UI** dev server with **CORS** to API (see [HOSTING.md §13](HOSTING.md#13-production-and-first-run-checklist)); v1 real-time is **SSE**, not WebSocket
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
| **1 – Minimal loop** | One task end-to-end | **Chat** workflow first; one worker; clone, run, commit to main; logs to control plane. |
| **2 – CLI & UI** | User-facing control | CLI: start session, list, tail logs. Web: same + attach. Session attach from both. |
| **3 – Loops & discovery** | Loops and multi-worker | Loop N and loop-until-sentinel; second worker; auto-registration. |
| **4 – Inboxes** | Continuous agents | Inbox per agent; continuous worker; spawn task to another agent’s inbox. |
| **5 – Polish** | Log search, PR/MR, secrets | Retention, search, PR/MR creation, secrets management (as in feature list). |

Adjust order and scope to match your priorities.

---

## 6. Communication & Docs

- [ ] **README** at repo root: what this is, how to run server/worker/CLI/UI, link to docs
- [ ] **Docs** in `docs/`: [README / index](README.md), ARCHITECTURE, TECH_STACK, PRODUCT, API_OVERVIEW, HOSTING, CLIENT_EXPERIENCE, TROUBLESHOOTING, this kickoff
- [ ] **API**: OpenAPI 3.x **checked into the repo** (path agreed with server crate); CI **fails** on drift vs handlers or generated types ([API_OVERVIEW.md — Spec delivery](API_OVERVIEW.md)). Document **SSE** shapes in OpenAPI descriptions or companion `docs/SSE_EVENTS.md`—state which in the OpenAPI README header.
- [ ] **Changelog**: deferred until formal releases

---

## 6a. Implementation plan (concrete checkpoints)

Use this as the **build order** for v1; details live in the linked specs.

| Order | Checkpoint | Spec / acceptance |
|-------|------------|-------------------|
| A | **Worker register** sends `client_version`; server enforces `worker_version_incompatible` | [API_OVERVIEW §9 — Register](API_OVERVIEW.md), [CLIENT_EXPERIENCE §13](CLIENT_EXPERIENCE.md#13-compatibility-and-upgrades) |
| B | **Loop until sentinel** = **literal substring** only; same semantics in engine, worker, and docs | [PRODUCT W3](PRODUCT.md), [API_OVERVIEW — Create session](API_OVERVIEW.md) |
| C | **Chat history cap** + `history_truncated` on pull payload; Settings/copy for long sessions | [API_OVERVIEW — Pull task](API_OVERVIEW.md#pull-task), [CLIENT_EXPERIENCE §12](CLIENT_EXPERIENCE.md#12-long-chat-sessions) |
| D | **Git clone** follows [GIT_CLONE_SPEC.md](GIT_CLONE_SPEC.md) (no libgit2 redirect replay regressions) | Worker `git_ops` checklist |
| E | **Session/job UI**: `error_message`, no silent “missing MR”; PR expectation copy | [PRODUCT I4](PRODUCT.md), [CLIENT_EXPERIENCE §8](CLIENT_EXPERIENCE.md#8-git-commit-push-and-prmr-outcomes), [Architecture §9a–9b](ARCHITECTURE.md#9a-when-the-worker-attempts-commit-and-push) |
| F | **Workers list** banner for mixed `platform` / heterogeneous pool | [CLIENT_EXPERIENCE §10](CLIENT_EXPERIENCE.md#10-worker-pool-heterogeneity-warnings), [ARCHITECTURE §4c](ARCHITECTURE.md#4c-platform-specific-workers-cli-invocation) |
| G | **Log retention** surfaced in UI; `retain_forever` discoverable | [PRODUCT L5](PRODUCT.md), [CLIENT_EXPERIENCE §9](CLIENT_EXPERIENCE.md#9-log-retention-and-purge) |
| H | **Bootstrap** gated per CLIENT_EXPERIENCE; operators follow [HOSTING §13–14](HOSTING.md#13-production-and-first-run-checklist) | Security UX |
| I | **CLI** human-readable errors only in v1 (**no `--json`** until spec + full coverage) | [API_OVERVIEW — Spec delivery](API_OVERVIEW.md) |
| J | **CI** green for Rust + Web + OpenAPI/schema drift | [CICD_DESIGN.md §4](CICD_DESIGN.md#4-not-yet-decided) |

---

## 7. Next Actions

1. **Create Cargo workspace** and crate layout (`server`, `worker`, `cli`, `api-types`).
2. **Implement Phase 0**: get control plane and one worker talking (register with `client_version`, one task, logs).
3. **Check in OpenAPI** and **SSE** documentation; wire CI so contracts do not drift.
4. **Implement first workflow** (chat) with Git commit, **history cap**, and **sentinel** semantics per API.
5. **Add CLI and Web UI** for start session + tail logs + attach + **§6a UX checkpoints** (workers banner, outcomes, retention, bootstrap).

---

*Navigation:* [Docs index](README.md) · [Architecture](ARCHITECTURE.md) · [Tech Stack](TECH_STACK.md) · [Product](PRODUCT.md) · [Client experience](CLIENT_EXPERIENCE.md)
