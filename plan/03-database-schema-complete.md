# 03 - Database Schema & Migrations

## Goal
Design and implement the full PostgreSQL schema via SQLx migrations. This is upfront design work — the schema must support all v1 features (sessions, jobs, workers, identities, API keys, personas, logs, inboxes).

## What to build

### Migration files in `crates/server/migrations/`

**Table: api_keys**
- `id` UUID PK
- `key_hash` TEXT NOT NULL (SHA-256 of the plain key)
- `label` TEXT
- `created_at` TIMESTAMPTZ NOT NULL DEFAULT now()

**Table: identities**
- `id` TEXT PK (default: "default")
- `agent_token` TEXT (encrypted or plain for v1)
- `git_token` TEXT
- `refresh_token` TEXT
- `token_expires_at` TIMESTAMPTZ
- `git_provider` TEXT (oauth_github, oauth_gitlab, manual)
- `git_base_url` TEXT (for self-hosted GitLab)
- `created_at` TIMESTAMPTZ NOT NULL DEFAULT now()
- `updated_at` TIMESTAMPTZ NOT NULL DEFAULT now()

**Table: personas**
- `id` UUID PK DEFAULT gen_random_uuid()
- `name` TEXT NOT NULL
- `prompt` TEXT NOT NULL
- `created_at` TIMESTAMPTZ NOT NULL DEFAULT now()
- `updated_at` TIMESTAMPTZ NOT NULL DEFAULT now()

**Table: workers**
- `id` TEXT PK
- `host` TEXT NOT NULL
- `labels` JSONB NOT NULL DEFAULT '{}'
- `capabilities` JSONB NOT NULL DEFAULT '[]'
- `client_version` TEXT
- `last_seen_at` TIMESTAMPTZ NOT NULL DEFAULT now()
- `created_at` TIMESTAMPTZ NOT NULL DEFAULT now()

**Table: sessions**
- `id` UUID PK DEFAULT gen_random_uuid()
- `repo_url` TEXT NOT NULL
- `ref` TEXT NOT NULL DEFAULT 'main'
- `workflow` TEXT NOT NULL
- `params` JSONB NOT NULL DEFAULT '{}'
- `persona_id` UUID REFERENCES personas(id)
- `identity_id` TEXT NOT NULL DEFAULT 'default' REFERENCES identities(id)
- `status` TEXT NOT NULL DEFAULT 'pending'
- `retain_forever` BOOLEAN NOT NULL DEFAULT false
- `created_at` TIMESTAMPTZ NOT NULL DEFAULT now()
- `updated_at` TIMESTAMPTZ NOT NULL DEFAULT now()

**Table: jobs**
- `id` UUID PK DEFAULT gen_random_uuid()
- `session_id` UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE
- `worker_id` TEXT REFERENCES workers(id)
- `status` TEXT NOT NULL DEFAULT 'pending' (pending, assigned, running, completed, failed)
- `task_input` JSONB NOT NULL DEFAULT '{}'
- `error_message` TEXT
- `branch` TEXT
- `commit_ref` TEXT
- `mr_title` TEXT
- `mr_description` TEXT
- `pull_request_url` TEXT
- `output` TEXT
- `sentinel_reached` BOOLEAN DEFAULT false
- `assistant_reply` TEXT
- `reclaim_count` INTEGER NOT NULL DEFAULT 0
- `assigned_at` TIMESTAMPTZ
- `created_at` TIMESTAMPTZ NOT NULL DEFAULT now()
- `updated_at` TIMESTAMPTZ NOT NULL DEFAULT now()

**Table: logs**
- `id` UUID PK DEFAULT gen_random_uuid()
- `session_id` UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE
- `job_id` UUID REFERENCES jobs(id) ON DELETE CASCADE
- `worker_id` TEXT
- `level` TEXT NOT NULL DEFAULT 'info'
- `source` TEXT NOT NULL DEFAULT 'control_plane'
- `message` TEXT NOT NULL
- `timestamp` TIMESTAMPTZ NOT NULL DEFAULT now()

**Indexes:**
- `logs`: composite on (session_id, timestamp), on (job_id, timestamp)
- `jobs`: on (session_id), on (status), on (worker_id, status)
- `workers`: on (last_seen_at)
- `sessions`: on (status, created_at)

**Seed data:**
- Insert default identity: `INSERT INTO identities (id) VALUES ('default')`

### Server startup migration runner
- In `crates/server/src/main.rs`, add `sqlx::migrate!("./migrations").run(&pool).await` before binding the HTTP listener (per ARCHITECTURE.md §2a)

## Dependencies
- Task 01 (repo scaffolding)

## Design decisions
- Use TIMESTAMPTZ everywhere (not TIMESTAMP) for timezone safety
- JSON columns (params, labels, capabilities, task_input) use JSONB for indexing capability
- Log retention via scheduled cleanup query (not DB TTL) — retention logic in a later task
- No inbox tables yet (P1 feature) — add when implementing inbox workflow
- jobs.status uses TEXT not enum for flexibility during development

## Test criteria
- [ ] `docker compose up -d postgres` starts PostgreSQL
- [ ] Server starts and runs migrations successfully: `cargo run -p server` logs "migrations applied"
- [ ] All tables exist: connect to DB and verify with `\dt`
- [ ] Default identity "default" is seeded
- [ ] Running server a second time is idempotent (no migration errors)
- [ ] `docker compose down -v && docker compose up -d postgres && cargo run -p server` does a clean re-creation
