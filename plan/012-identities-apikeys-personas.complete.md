# 012: Identities, API Keys & Personas — Full CRUD

## Goal
Complete the identity/credentials management (including auth-status), API key lifecycle (create, list, revoke, bootstrap), and personas CRUD. These are all relatively small endpoints that share a "settings/admin" theme.

## Scope
### Identities (server)
- `GET /identities/:id` — Return has_git_token, has_agent_token (already partially done). 404 if not found.
- `GET /identities/:id/auth-status` — Return git_token_status (healthy/expiring_soon/expired_refreshable/expired_needs_reauth/unknown/not_configured), git_provider, token_expires_at, message. Check token_expires_at against current time.
- `GET /identities/:id/repositories` — Call GitHub or GitLab API with the stored git_token. Return list of repos (full_name, clone_url). Detect provider from git_provider field or ?provider query param. Handle 401/502 from provider.
- `PATCH /identities/:id` — Update tokens (already partially done). Only update provided fields.

### API Keys (server)
- `POST /api-keys/bootstrap` — No auth. Create first key only when no keys exist (env or DB). Return 403 if any key exists.
- `POST /api-keys` — Create new API key. Generate random key, store SHA-256 hash. Return plain key once.
- `GET /api-keys` — List keys (id, label, created_at). Paginated. No secrets.
- `DELETE /api-keys/:id` — Revoke key. 204. Key stops working immediately.
- Auth validation: check incoming key against env keys AND DB-issued key hashes.

### Personas (server)
- `POST /personas` — Create persona (name, prompt). Return 201 with persona_id.
- `GET /personas` — List personas (paginated). Omit prompt in list for brevity.
- `GET /personas/:id` — Get persona with prompt. 404 if not found.
- `PATCH /personas/:id` — Update name and/or prompt. 204.
- `DELETE /personas/:id` — Remove persona. 204.

### CLI additions
- `remote-harness api-key create [--label <label>]` — POST /api-keys, print key (show once warning)
- `remote-harness api-key list` — GET /api-keys, tabular
- `remote-harness api-key revoke <id>` — DELETE /api-keys/:id

### Web UI additions
- Settings page: "API Keys" section with create (shows key once in modal), list, revoke
- Settings page: show auth-status info (token health, expiry)
- Session create: persona selector (dropdown from GET /personas)

## Prerequisites
- Spec 001 (foundation, DB schema includes api_keys, personas, identities tables)
- Spec 007 (CLI core)
- Spec 009 (Web UI scaffold, settings page)

## Files to create/modify
- `crates/server/src/routes/identities.rs` — New: get, auth-status, repositories, update
- `crates/server/src/routes/api_keys.rs` — New: bootstrap, create, list, revoke
- `crates/server/src/routes/personas.rs` — New: CRUD
- `crates/server/src/routes/mod.rs` — Mount new routes
- `crates/server/src/auth.rs` — Add DB-issued key validation (hash lookup)
- `crates/cli/src/commands/api_keys.rs` — New
- `crates/cli/src/main.rs` — Add api-key subcommands
- `web/src/pages/Settings.tsx` — Add API keys section, auth status display

## Acceptance criteria
1. `GET /identities/default/auth-status` → correct status based on token state
2. `GET /identities/default/repositories` → repo list from GitHub/GitLab (or 400 if no provider)
3. `POST /api-keys/bootstrap` → 201 when no keys exist, 403 when keys exist
4. `POST /api-keys` → 201 with plain key; key works for auth; stored as hash
5. `GET /api-keys` → list without secrets
6. `DELETE /api-keys/:id` → key stops working
7. Persona CRUD works (create, list, get, update, delete)
8. CLI api-key commands work end-to-end
9. Web UI settings shows API key management
10. Web UI session create has persona selector
11. `cargo test` — at least 5 tests (bootstrap logic, key validation, persona CRUD)
12. `cargo clippy` clean, `npm run build` succeeds

## Implementation notes
- API key generation: 32 random bytes → hex or base64url. Store SHA-256 hash in DB.
- Auth check: first check env API_KEY/API_KEYS (fast, in-memory). If no match, hash incoming key and query api_keys table.
- Bootstrap check: count env keys + DB keys. If total = 0, allow bootstrap.
- For repositories endpoint: make HTTP call to `https://api.github.com/user/repos` or `https://gitlab.com/api/v4/projects` (or git_base_url) with the stored token.
