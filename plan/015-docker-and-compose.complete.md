# 015: Dockerfile & Docker Compose

## Goal
The entire stack (Postgres, server, worker, web UI) runs with a single `docker compose up --build`. Follows GETTING_STARTED_WITH_DOCKER.md exactly.

## Scope
### Root Dockerfile (server + worker)
- Multi-stage build:
  1. Builder stage: rust image, copy workspace, `cargo build --release -p server -p worker`
  2. Runtime stage: slim Debian, copy server + worker binaries, install Cursor Agent CLI (official install script)
- Server command: `./server`
- Worker command: `./worker`

### web/Dockerfile
- Stage 1: Node image, `npm ci`, `npm run build`
- Stage 2: nginx:alpine, copy built files, configure SPA routing (`try_files $uri $uri/ /index.html`)
- Expose port 80

### docker-compose.yml
Services:
- `postgres`: postgres:16, healthcheck, volume for data, env for user/password/db
- `server`: build from root Dockerfile, command: `./server`, depends_on postgres (healthy), env: DATABASE_URL, API_KEY, CORS_ALLOWED_ORIGINS, HOST=0.0.0.0, PORT=3000. Ports: 3000:3000. Restart: unless-stopped.
- `worker`: same image as server, command: `./worker`, depends_on server, env: CONTROL_PLANE_URL=http://server:3000, REMOTE_HARNESS_API_KEY=${API_KEY}. Restart: unless-stopped.
- `web`: build from web/Dockerfile, ports: 5173:80, depends_on server. Restart: unless-stopped.

### .env.example
```
API_KEY=dev-key-change-in-production
DATABASE_URL=postgres://postgres:postgres@postgres:5432/remote_harness
CORS_ALLOWED_ORIGINS=http://localhost:5173,http://127.0.0.1:5173
```

### nginx.conf for web
```
server {
    listen 80;
    root /usr/share/nginx/html;
    index index.html;
    location / {
        try_files $uri $uri/ /index.html;
    }
}
```

## Prerequisites
- All Rust crates build (specs 001-008)
- Web UI builds (specs 009-010)

## Files to create/modify
- `Dockerfile` — Root, multi-stage for server + worker
- `web/Dockerfile` — Web UI build + nginx
- `web/nginx.conf` — SPA routing
- `docker-compose.yml` — Full stack
- `.env.example` — Template
- `.dockerignore` — Exclude target/, node_modules/, .git/

## Acceptance criteria
1. `docker compose up --build` starts all 4 services
2. Postgres healthcheck passes before server starts
3. Server runs migrations and listens on :3000
4. Worker registers with server
5. Web UI accessible at http://localhost:5173
6. `curl http://localhost:3000/health` → ok
7. `docker compose down -v` cleans up (including DB volume)
8. `docker compose up --scale worker=2` runs 2 workers
9. Services restart after crash (unless-stopped)

## Implementation notes
- The builder stage should cache cargo dependencies by copying Cargo.toml/Cargo.lock first, then `cargo build` (dependency layer), then copy src and rebuild (fast rebuilds).
- Server connects to postgres via `DATABASE_URL=postgres://postgres:postgres@postgres:5432/remote_harness` (the compose service name `postgres` is the hostname inside the Docker network).
- Worker connects to server via `http://server:3000` (compose service name).
- CORS must include `http://localhost:5173` for the web UI.
