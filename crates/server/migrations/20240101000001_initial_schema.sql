-- Initial schema for Remote Harness v1

-- API Keys
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key_hash TEXT NOT NULL,
    label TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Identities
CREATE TABLE identities (
    id TEXT PRIMARY KEY DEFAULT 'default',
    agent_token TEXT,
    git_token TEXT,
    refresh_token TEXT,
    token_expires_at TIMESTAMPTZ,
    git_provider TEXT,
    git_base_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Personas
CREATE TABLE personas (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    prompt TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Workers
CREATE TABLE workers (
    id TEXT PRIMARY KEY,
    host TEXT NOT NULL,
    labels JSONB NOT NULL DEFAULT '{}',
    capabilities JSONB NOT NULL DEFAULT '[]',
    client_version TEXT,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Sessions
CREATE TABLE sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repo_url TEXT NOT NULL,
    ref TEXT NOT NULL DEFAULT 'main',
    workflow TEXT NOT NULL,
    params JSONB NOT NULL DEFAULT '{}',
    persona_id UUID REFERENCES personas(id),
    identity_id TEXT NOT NULL DEFAULT 'default' REFERENCES identities(id),
    status TEXT NOT NULL DEFAULT 'pending',
    retain_forever BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Jobs
CREATE TABLE jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    worker_id TEXT REFERENCES workers(id),
    status TEXT NOT NULL DEFAULT 'pending',
    task_input JSONB NOT NULL DEFAULT '{}',
    error_message TEXT,
    branch TEXT,
    commit_ref TEXT,
    mr_title TEXT,
    mr_description TEXT,
    pull_request_url TEXT,
    output TEXT,
    sentinel_reached BOOLEAN DEFAULT false,
    assistant_reply TEXT,
    reclaim_count INTEGER NOT NULL DEFAULT 0,
    assigned_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Logs
CREATE TABLE logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    job_id UUID REFERENCES jobs(id) ON DELETE CASCADE,
    worker_id TEXT,
    level TEXT NOT NULL DEFAULT 'info',
    source TEXT NOT NULL DEFAULT 'control_plane',
    message TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes
CREATE INDEX idx_logs_session_timestamp ON logs (session_id, timestamp);
CREATE INDEX idx_logs_job_timestamp ON logs (job_id, timestamp);
CREATE INDEX idx_jobs_session_id ON jobs (session_id);
CREATE INDEX idx_jobs_status ON jobs (status);
CREATE INDEX idx_jobs_worker_status ON jobs (worker_id, status);
CREATE INDEX idx_workers_last_seen ON workers (last_seen_at);
CREATE INDEX idx_sessions_status_created ON sessions (status, created_at);

-- Seed default identity
INSERT INTO identities (id) VALUES ('default');
