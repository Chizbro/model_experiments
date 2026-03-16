# Open Points and Decisions

Quick reference for what is still **TBD**, **ambiguous**, or **optional** after the clarification pass. Use this to decide anything you care about before implementation.

---

## 1. Explicitly TBD (no decision needed until you use them)

| Item | Where | When to decide |
|------|--------|----------------|
| **CI platform** | DECISIONS §8b | When you add CI (e.g. GitHub Actions, GitLab CI). |
| **Git host** | DECISIONS §8c | When you host the repo (e.g. GitHub, GitLab). |

No product impact; design is platform-agnostic.

---

## 2. Small ambiguities (decisions that would lock implementation)

These are the only remaining “could go either way” bits. You can leave them to implementation and document the choice later, or decide now.

### 2.1 Task payload to worker: how persona + task is represented — **decided**

**Decision (DECISIONS §26):** Two parts — **prompt_context** (persona prompt; omitted/empty when no persona) and **task_input** (task-specific input). Worker passes prompt_context as agent context (e.g. system prompt) and task_input as user/task input to the CLI. See [API_OVERVIEW §9](API_OVERVIEW.md) (Pull task).


---

### 2.2 Session create: default for `ref` — **decided**

**Decision (DECISIONS §27):** If the client omits **ref**, the server uses **main**. See [API_OVERVIEW §4](API_OVERVIEW.md).


---

### 2.3 Log history: how much to load before streaming — **decided**

**Decision (DECISIONS §28):** Load **all** log history; **no cap**. Paginate until no `next_cursor`, then render and attach the stream. See [API_OVERVIEW §6](API_OVERVIEW.md), [Architecture §6](ARCHITECTURE.md).


---

### 2.4 Platform affinity for task assignment — **decided**

**Decision (DECISIONS §29):** **v1: no platform affinity.** Engine assigns to any available worker. Workers still advertise `platform` for observability (UI filtering, display). See [Architecture §4c](ARCHITECTURE.md), [API_OVERVIEW §9](API_OVERVIEW.md).

---

## 3. Intentionally optional / later

No decision required unless you want to lock scope.

- **Persona PATCH/DELETE** — API_OVERVIEW says “optional in v1.” Implement or defer.
- **Sentinel regex** — v1: literal only; regex later.
- **OIDC / mTLS** — v1: API key only; add later if needed.
- **WebSocket for log tail** — v1: SSE only; add later if needed.
- **DB LISTEN/NOTIFY for worker task push** — v1: poll only.

---

## 4. Doc housekeeping

**DOC_CLARIFICATION_NEEDED.md** still lists some items in §1, §2.2, §2.3, §2.5, §2.6 and §3 as open; those were actually resolved by DECISIONS §13–§22 (and §23–§25). Updating that doc to mark them “addressed” would avoid confusion. The only *real* open design choices are in **§2** above.

---

## Summary

- **Must decide before coding:** Nothing blocking. You can start with the current docs.
- **Nice to decide for consistency:** §2.1 (task payload shape), §2.2 (default `ref`), §2.3 (log history cap), §2.4 (platform affinity). All have reasonable defaults if you leave them to implementation.
- **Decide when relevant:** §1 (CI platform, Git host).
