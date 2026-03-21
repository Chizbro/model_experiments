-- Agents and inbox task queue (docs/API_OVERVIEW.md §8, PHASE2_DESIGN.md §3).

CREATE TABLE agents (
    id TEXT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE inbox_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id TEXT NOT NULL REFERENCES agents (id) ON DELETE CASCADE,
    payload JSONB NOT NULL,
    persona_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    promoted_job_id UUID,
    enqueued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT inbox_tasks_status_chk CHECK (status IN ('pending', 'promoted'))
);

CREATE INDEX inbox_tasks_agent_pending_idx ON inbox_tasks (agent_id, enqueued_at, id)
WHERE
    status = 'pending';

-- At most one active listener registration per agent (worker_id is the consuming worker).
CREATE TABLE inbox_listeners (
    agent_id TEXT PRIMARY KEY REFERENCES agents (id) ON DELETE CASCADE,
    worker_id TEXT NOT NULL REFERENCES workers (id) ON DELETE CASCADE,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX inbox_listeners_worker_idx ON inbox_listeners (worker_id);
