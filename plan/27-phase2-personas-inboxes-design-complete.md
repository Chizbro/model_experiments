# 27 — Phase 2 design backlog: personas, inboxes, PR/MR, search

**Status:** complete  
**Dependencies:** 26 (after v1 path is stable)

## Objective

**Design-first** backlog for P1 features so they land with the same **clean boundaries** as v1 ([PRODUCT](../docs/PRODUCT.md) W4–W6, O2–O4, L5 search). Produce **ADR or spec deltas** before large implementation PRs.

## Scope

**In scope**

- **Personas** ([API_OVERVIEW §5a](../docs/API_OVERVIEW.md#5a-rest--personas), [Architecture §4b](../docs/ARCHITECTURE.md#4b-personas-separate-agent-identities)): resolution order, storage, UI CRUD.
- **Inboxes** ([API_OVERVIEW §8](../docs/API_OVERVIEW.md#8-rest--inboxes-p1)): worker pull integration with continuous sessions; cross-agent spawn.
- **PR/MR creation** ([Architecture §9b](../docs/ARCHITECTURE.md#9b-when-the-control-plane-creates-a-prmr)): provider matrix, token scopes, failure UX.
- **Log search** (P1): index choice vs Postgres `LIKE` feasibility.

**Out of scope**

- Actual implementation—split into future `plan/28-*` tasks after this design is approved.

## Spec references

- [PRODUCT — Optional / Later](../docs/PRODUCT.md#optional--later)
- [API_OVERVIEW §8, §5a](../docs/API_OVERVIEW.md)

## Acceptance criteria

- Written design merged under `docs/` (new file or sections) with **open questions** resolved or explicitly flagged.
- No implementation required in this task.

## Testing

| When | What | Retest |
|------|------|--------|
| After design | Review with checklist vs PRODUCT priorities | Before Phase 2 coding |

---

## Completed / Notes

- Added canonical design doc **[docs/PHASE2_DESIGN.md](../docs/PHASE2_DESIGN.md)** (personas §2, inboxes §3, PR/MR §4, log search §5; open questions in tables).
- Cross-linked **docs/README.md**, **PRODUCT.md** (Optional / Later), **ARCHITECTURE.md** (§4b, §7, §9b), **API_OVERVIEW.md** (§5a, §6, §8).
- Implementation remains for future **plan/28-*** tasks per original scope.
