# 05 - API Key Auth System

## Goal
Implement the full API key lifecycle (bootstrap, create, list, revoke) and the auth middleware that protects all authenticated endpoints. This is auth concern 1 (control plane auth) — completely separate from Git/agent tokens.

## What to build

### Auth middleware (`crates/server/src/middleware/auth.rs`)
- Extract API key from `Authorization: Bearer <key>` or `X-API-Key: <key>` header
- Check against: (1) env-based keys from `API_KEY`/`API_KEYS` config, (2) DB-issued keys (hash comparison)
- Hash incoming key with SHA-256 and compare to stored `key_hash` in `api_keys` table
- Return `401 { "error": { "code": "unauthorized", "message": "..." } }` on failure
- Apply to all routes except: `/health`, `/ready`, `/health/idle`, `/auth/*`, `/api-keys/bootstrap`

### API key routes (`crates/server/src/routes/api_keys.rs`)

**POST /api-keys/bootstrap**
- No auth required
- Check if ANY keys exist (env + DB). If yes, return `403 { "code": "forbidden", "message": "Keys already exist" }`
- Generate random key, hash it, store hash in DB, return plain key once
- Response: `201 { "id", "key", "label", "created_at" }`

**POST /api-keys** (authenticated)
- Generate random key (e.g. `rh_` prefix + 32 random bytes base62)
- Store SHA-256 hash in `api_keys` table
- Return plain key in response (only time it's shown)
- Response: `201 { "id", "key", "label", "created_at" }`

**GET /api-keys** (authenticated)
- Paginated list of keys (id, label, created_at — no secret)
- Response: `200 { "items": [...], "next_cursor": ... }`

**DELETE /api-keys/:id** (authenticated)
- Delete key from DB. `204` on success, `404` if not found.
- Key stops working immediately.

### Key generation utility
- Generate cryptographically random keys with recognizable prefix (e.g. `rh_...`)
- SHA-256 hashing function for storage

## Dependencies
- Task 04 (server foundation — axum router, DB pool, error handling)

## Test criteria
- [ ] Without any keys configured: `POST /api-keys/bootstrap` returns `201` with a key
- [ ] After bootstrap: `POST /api-keys/bootstrap` returns `403`
- [ ] Authenticated request with valid key returns `200`
- [ ] Request with invalid/missing key returns `401`
- [ ] `POST /api-keys` creates a new key (returned once)
- [ ] `GET /api-keys` lists keys without secrets
- [ ] `DELETE /api-keys/:id` revokes key, subsequent auth with that key returns `401`
- [ ] Env-based `API_KEY` works without any DB keys
- [ ] Both `Authorization: Bearer` and `X-API-Key` headers accepted
- [ ] Integration tests for the full key lifecycle
- [ ] `cargo test -p server` passes
