# 26 — E2E: Docker Compose smoke and success criteria

**Status:** complete  
**Dependencies:** 01–19 minimum; 21–25 for full product checks

## Objective

Prove [PRODUCT — Success criteria](../docs/PRODUCT.md#success-criteria-early-phase) and [PROJECT_KICKOFF phases](../docs/PROJECT_KICKOFF.md#5-phases--milestones-approved) with **one documented** Compose stack: control plane + Postgres + worker + (optional) web build served statically.

## Scope

**In scope**

- Script or `justfile`/`Makefile` target: `compose up`, wait for healthy, **bootstrap key**, set identity tokens (fixture or `.env.example`), run **one chat session** (may use stub agent in CI vs real CLI locally—**two tiers** documented).
- Assert: **chat** job reaches **`completed`** (session may stay **`running`** for follow-up—see server `complete_task`); logs visible via API; commit optional depending on environment.
- Root **README** complete: how to run server, worker, CLI, UI ([PROJECT_KICKOFF §6](../docs/PROJECT_KICKOFF.md#6-communication--docs)).

**Out of scope**

- Hosted CI E2E secrets policy—document what runs in PR vs nightly.

## Spec references

- [GETTING_STARTED](../docs/GETTING_STARTED.md)
- [TROUBLESHOOTING](../docs/TROUBLESHOOTING.md)

## Acceptance criteria

- A new contributor can follow README and see a **green path** in under X minutes (state assumptions).
- Optional GitHub Actions job marked `workflow_dispatch` or nightly for full E2E.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | Run smoke script locally + CI subset | On release candidates |

---

## Completed / Notes

- **`scripts/compose-smoke.sh`**: Compose up (with `docker-compose.smoke.yml`: stub agent, bind-mounted bare repo, worker `user: "0:0"` for push permissions), wait `/health`, PATCH default identity fixture tokens, `POST /sessions` chat on `file:///e2e/repo.git`, poll until **`jobs[0].status === "completed"`** (session may remain **`running`** for chat), assert `GET .../logs` has entries. Teardown unless `RH_SMOKE_KEEP_STACK=1`. Optional **`RH_SMOKE_BOOTSTRAP=1`** + `docker-compose.smoke-bootstrap.yml` for bootstrap-first flow.
- **`docker-compose.yml`**: Added **`web`** service (Vite build + nginx from `web/Dockerfile`).
- **Docs:** Root `README.md`, `docs/GETTING_STARTED.md` §1.9, `docs/PRODUCT.md` success criteria note, `docs/CICD_DESIGN.md` §2.3, `docs/TROUBLESHOOTING.md` §1e, `.env.example`.
- **CI:** `.github/workflows/e2e-compose.yml` — `workflow_dispatch` + nightly schedule.
