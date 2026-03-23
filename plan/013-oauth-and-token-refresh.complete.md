# 013: OAuth (GitHub & GitLab) + Token Refresh

## Goal
Users can sign in with GitHub or GitLab via the Web UI to store git tokens. The server handles OAuth flows with CSRF + PKCE, stores tokens, and auto-refreshes expired tokens.

## Scope
### Server OAuth endpoints
- `GET /auth/github` — Start GitHub OAuth. Generate CSRF nonce + PKCE code_verifier. Store in HttpOnly cookie `_rh_oauth`. Redirect to GitHub authorization URL with code_challenge (S256).
- `GET /auth/github/callback` — Exchange code for token. Validate CSRF nonce from cookie vs state param. Store git_token on identity. Clear cookie. Redirect to REDIRECT_AFTER_AUTH.
- `GET /auth/gitlab` — Same flow for GitLab. Use GITLAB_BASE_URL for self-hosted.
- `GET /auth/gitlab/callback` — Same as GitHub. Also store refresh_token, token_expires_at, git_base_url.

### Token refresh
- Before serving git_token to a worker (in pull_task) or using it for PR/MR creation: check token_expires_at. If expired or expiring within 5 minutes, and refresh_token exists, call provider's token refresh endpoint. Update identity with new token, expiry, and refresh_token.
- If refresh fails and token is expired: task pull should still include the (expired) token but log a warning. The job will fail with an auth error — surface to user.

### Server config
- Env vars: GITHUB_CLIENT_ID, GITHUB_CLIENT_SECRET, GITHUB_REDIRECT_URI, GITLAB_CLIENT_ID, GITLAB_CLIENT_SECRET, GITLAB_REDIRECT_URI, GITLAB_BASE_URL, REDIRECT_AFTER_AUTH
- OAuth routes skip API key auth (they're browser redirect flows)

### Web UI
- Settings page: "Sign in with GitHub" / "Sign in with GitLab" buttons. These navigate the browser to /auth/github or /auth/gitlab on the control plane. After callback, browser redirects back to Settings with ?credentials=github_ok or similar. Parse query param and show success toast.
- Only show OAuth buttons if configured (server reports this somehow — simplest: try the URL and if 503, hide the button; or add a /config endpoint).

## Prerequisites
- Spec 012 (identities endpoints)
- Spec 009 (Web UI settings)

## Files to create/modify
- `crates/server/src/routes/oauth.rs` — New: github, github/callback, gitlab, gitlab/callback
- `crates/server/src/routes/mod.rs` — Mount OAuth routes (no auth middleware)
- `crates/server/src/config.rs` — Add OAuth env vars
- `crates/server/src/engine/mod.rs` — Token refresh logic in pull_task path
- `web/src/pages/Settings.tsx` — OAuth buttons and callback handling

## Acceptance criteria
1. `GET /auth/github` redirects to GitHub OAuth (when configured)
2. `GET /auth/github` returns 503 when not configured
3. GitHub callback stores git_token on identity and redirects to UI
4. GitLab flow works the same (with PKCE + CSRF)
5. CSRF validation: callback with mismatched state → 400
6. Token refresh: expired GitLab token with refresh_token → auto-refreshed on pull_task
7. Token refresh failure: logged, task proceeds with expired token
8. Web UI: OAuth buttons shown when configured, hidden when not
9. Web UI: success toast after OAuth callback redirect
10. `cargo test` — at least 3 tests (PKCE generation, CSRF validation, refresh logic)
11. `cargo clippy` clean

## Implementation notes
- PKCE: generate 32-byte random code_verifier, compute SHA-256 → base64url as code_challenge. Store code_verifier in cookie.
- CSRF: include random nonce in state param (e.g., `{nonce}:{identity_id}`), store nonce in cookie.
- Cookie: `Set-Cookie: _rh_oauth={json}; HttpOnly; SameSite=Lax; Path=/auth; Max-Age=600`
- GitHub token endpoint: POST https://github.com/login/oauth/access_token
- GitLab token endpoint: POST {GITLAB_BASE_URL}/oauth/token (default: https://gitlab.com/oauth/token)
- GitLab refresh: POST same endpoint with grant_type=refresh_token
