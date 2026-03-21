# 23 — Web: sessions and workers views

**Status:** complete  
**Dependencies:** 22, 11, 09

## Objective

**Dashboard**: list/create session; session detail with jobs; workers list with **heterogeneity banner** when ≥2 workers differ on `labels.platform` or WSL vs native Windows ([CLIENT_EXPERIENCE §10](../docs/CLIENT_EXPERIENCE.md#10-worker-pool-heterogeneity-warnings), [PROJECT_KICKOFF §6a — F](../docs/PROJECT_KICKOFF.md#6a-implementation-plan-concrete-checkpoints)).

## Scope

**In scope**

- Paginated lists per API cursor pattern.
- Session create form: workflow, repo, identity, `agent_cli`, branch options—reject with link to Settings on missing tokens ([CLIENT_EXPERIENCE §5](../docs/CLIENT_EXPERIENCE.md#5-credentials-and-byol)).
- Optional confirm when `agent_cli` likely incompatible with pool (heuristic v1).

**Out of scope**

- Log SSE panel (24).

## Spec references

- [API_OVERVIEW §4–5](../docs/API_OVERVIEW.md)
- [ARCHITECTURE §4c](../docs/ARCHITECTURE.md#4c-platform-specific-workers-cli-invocation)

## Acceptance criteria

- Component tests or Playwright smoke for create → redirect to detail.
- Banner snapshot or unit test with mocked worker list data.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `npm test` / E2E optional | CI |
