# 27 - Docker & Docker Compose Deployment

## Goal
Create the multi-stage Dockerfile and docker-compose.yml for building and running the full stack: PostgreSQL, control plane server, worker, and web UI.

## What to build

### Dockerfile (repo root)
Multi-stage build:

**Stage 1: Rust builder**
- Base: `rust:latest` (or pinned version)
- Copy Cargo workspace
- Build release binaries: server, worker, cli
- Output: `/app/server`, `/app/worker`, `/app/cli`

**Stage 2: Web builder**
- Base: `node:20-alpine`
- Copy web/
- `npm ci && npm run build`
- Output: `/app/web/dist`

**Stage 3: Server runtime**
- Base: `debian:bookworm-slim` (or alpine with required libs for libgit2)
- Copy server binary from stage 1
- Copy web/dist from stage 2 (serve static files from server or separate nginx)
- Install: libssl, ca-certificates, libgit2 system deps
- Entrypoint: `./server`

**Stage 4: Worker runtime**
- Base: `debian:bookworm-slim`
- Copy worker binary from stage 1
- Install: git, libssl, ca-certificates, libgit2 deps
- Note: Claude Code / Cursor CLI must be installed separately (BYOL)
- Entrypoint: `./worker`

### docker-compose.yml (repo root)
```yaml
services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: remote_harness
      POSTGRES_USER: harness
      POSTGRES_PASSWORD: harness_dev
    volumes:
      - pgdata:/var/lib/postgresql/data
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U harness"]
      interval: 5s
      timeout: 5s
      retries: 5

  server:
    build:
      context: .
      target: server-runtime
    depends_on:
      postgres:
        condition: service_healthy
    environment:
      DATABASE_URL: postgres://harness:harness_dev@postgres:5432/remote_harness
      PORT: 3000
      CORS_ALLOWED_ORIGINS: http://localhost:5173,http://localhost:3000
    ports:
      - "3000:3000"
    restart: unless-stopped

  worker:
    build:
      context: .
      target: worker-runtime
    depends_on:
      - server
    environment:
      CONTROL_PLANE_URL: http://server:3000
      API_KEY: ${API_KEY:-}
    restart: unless-stopped

  web:
    build:
      context: ./web
    ports:
      - "5173:80"
    # Or serve from server directly

volumes:
  pgdata:
```

### docker-compose.dev.yml (overrides for development)
- Mount source code volumes for hot-reload
- Use cargo-watch for server/worker
- Vite dev server for web

### .env.example
- Template with all environment variables documented

### Health check integration
- Server healthcheck: `GET /health`
- Postgres healthcheck: `pg_isready`
- Compose depends_on with health conditions

## Dependencies
- All server tasks (04-12) and worker tasks (13-16) should be complete
- Task 22-25 (web UI)

## Test criteria
- [ ] `docker compose build` succeeds
- [ ] `docker compose up` starts all services
- [ ] PostgreSQL starts and passes health check
- [ ] Server starts, runs migrations, passes health check
- [ ] Worker registers with server
- [ ] Web UI accessible on configured port
- [ ] `docker compose down -v` cleans up completely
- [ ] `docker compose up --scale worker=2` runs 2 workers
- [ ] `restart: unless-stopped` restarts crashed containers
- [ ] Environment variables configurable via .env file
