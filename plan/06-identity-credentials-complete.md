# 06 - Identity & BYOL Credentials

## Goal
Implement the identity system for storing and managing BYOL credentials (agent tokens for Claude Code/Cursor, and Git tokens for GitHub/GitLab). This covers auth concerns 2 (Git) and 3 (agent CLI) — storing tokens the user provides.

## What to build

### Identity routes (`crates/server/src/routes/identities.rs`)

**GET /identities/:id**
- Return credential status (never return actual token values)
- Response: `200 { "has_git_token": bool, "has_agent_token": bool }`
- `404` if identity not found

**GET /identities/:id/auth-status**
- Return token health info (expiry, refresh capability)
- Check `token_expires_at` against current time
- Response: `200 { "git_token_status": "healthy|expiring_soon|expired_refreshable|expired_needs_reauth|not_configured", "git_provider": "...", "token_expires_at": "...", "message": "..." }`
- `expiring_soon`: expires within 1 hour
- `expired_refreshable`: expired but has refresh_token
- `expired_needs_reauth`: expired, no refresh_token

**GET /identities/:id/repositories**
- Query param: `provider=github|gitlab`
- Use stored git_token to call GitHub/GitLab API for repo list
- GitHub: `GET https://api.github.com/user/repos`
- GitLab: `GET https://gitlab.com/api/v4/projects?membership=true` (or git_base_url)
- Response: `200 { "items": [{ "full_name": "owner/repo", "clone_url": "https://..." }], "provider": "github|gitlab" }`
- `400` if provider unknown, `401`/`502` if provider rejects token

**PATCH /identities/:id**
- Partial update: any subset of `{ agent_token, git_token, refresh_token }`
- Only provided fields are updated
- Response: `204` on success, `404` if not found

### Credential resolution for tasks
- Helper function: given a session's `identity_id` and session `params`, resolve the final `git_token` and `agent_token` (identity first, params override)
- This will be used by the task dispatch code (Task 08)

## Dependencies
- Task 04 (server foundation)
- Task 05 (API key auth — routes are authenticated)

## Test criteria
- [ ] Default identity "default" exists from migration seed
- [ ] `PATCH /identities/default` with `agent_token` and `git_token` stores them
- [ ] `GET /identities/default` returns `{ "has_git_token": true, "has_agent_token": true }` after patching
- [ ] Token values are never returned in any GET response
- [ ] `GET /identities/default/auth-status` returns correct status based on token_expires_at
- [ ] `GET /identities/default/repositories?provider=github` calls GitHub API (mock in tests)
- [ ] Credential resolution merges identity tokens with session params correctly
- [ ] `cargo test -p server` passes
