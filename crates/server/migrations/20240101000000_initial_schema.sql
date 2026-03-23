-- Workers
CREATE TABLE IF NOT EXISTS workers (
    id TEXT PRIMARY KEY,
    host TEXT NOT NULL,
    labels JSONB NOT NULL DEFAULT '{}',
    capabilities JSONB NOT NULL DEFAULT '[]',
    client_version TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Identities (BYOL credentials)
CREATE TABLE IF NOT EXISTS identities (
    id TEXT PRIMARY KEY,
    agent_token TEXT,
    git_token TEXT,
    refresh_token TEXT,
    token_expires_at TIMESTAMPTZ,
    git_provider TEXT,
    git_base_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed default identity
INSERT INTO identities (id) VALUES ('default') ON CONFLICT DO NOTHING;

-- Sessions
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    repo_url TEXT NOT NULL,
    ref_name TEXT NOT NULL DEFAULT 'main',
    workflow TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    params JSONB NOT NULL DEFAULT '{}',
    identity_id TEXT NOT NULL DEFAULT 'default' REFERENCES identities(id),
    persona_id TEXT,
    retain_forever BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Jobs (one per iteration/turn)
CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    worker_id TEXT REFERENCES workers(id),
    status TEXT NOT NULL DEFAULT 'pending',
    iteration_index INTEGER NOT NULL DEFAULT 0,
    task_input JSONB NOT NULL DEFAULT '{}',
    error_message TEXT,
    branch TEXT,
    commit_ref TEXT,
    mr_title TEXT,
    mr_description TEXT,
    pull_request_url TEXT,
    output TEXT,
    assistant_reply TEXT,
    sentinel_reached BOOLEAN NOT NULL DEFAULT FALSE,
    reclaim_count INTEGER NOT NULL DEFAULT 0,
    assigned_at TIMESTAMPTZ,
    retain_forever BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Logs (central store)
CREATE TABLE IF NOT EXISTS logs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    job_id TEXT,
    worker_id TEXT,
    level TEXT NOT NULL DEFAULT 'info',
    source TEXT NOT NULL DEFAULT 'control_plane',
    message TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_logs_session_timestamp ON logs(session_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_logs_job ON logs(job_id) WHERE job_id IS NOT NULL;

-- API keys (issued, hashed)
CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    key_hash TEXT NOT NULL,
    label TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Personas
CREATE TABLE IF NOT EXISTS personas (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    prompt TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Inbox tasks (P1, create table now for schema completeness)
CREATE TABLE IF NOT EXISTS inbox_tasks (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}',
    persona_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    enqueued_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
