# 07 - Git OAuth Flows (GitHub + GitLab)

## Goal
Implement browser-based OAuth sign-in for GitHub and GitLab so users can authenticate and have their git_token stored on an identity. Includes CSRF protection and PKCE (S256).

## What to build

### OAuth routes (`crates/server/src/routes/oauth.rs`)

**GET /auth/github**
- Query: optional `identity_id` (default "default")
- Generate random CSRF nonce + PKCE code_verifier
- Store nonce + code_verifier + identity_id in `_rh_oauth` HttpOnly, SameSite=Lax cookie
- Build GitHub authorization URL with: client_id, redirect_uri, scope (repo), state (nonce + identity_id), code_challenge (SHA-256 of code_verifier, base64url)
- Redirect (302) to GitHub
- Return `503` if `GITHUB_CLIENT_ID` not configured

**GET /auth/github/callback**
- Extract `code` and `state` from query
- Validate CSRF: nonce from `state` matches nonce in `_rh_oauth` cookie
- Exchange code for access token at `https://github.com/login/oauth/access_token` with code_verifier (PKCE)
- Store `git_token`, `git_provider: "oauth_github"` on the identity
- Clear `_rh_oauth` cookie
- Redirect to `REDIRECT_AFTER_AUTH` URL (e.g. UI settings page)

**GET /auth/gitlab**
- Same pattern as GitHub but for GitLab
- Uses `GITLAB_CLIENT_ID`, `GITLAB_REDIRECT_URI`, `GITLAB_BASE_URL` (default `https://gitlab.com`)
- GitLab returns refresh_token + expires_in — store both
- Store `git_base_url` on identity for self-hosted GitLab

**GET /auth/gitlab/callback**
- Same pattern: validate CSRF, exchange code with PKCE, store tokens
- Store `git_provider: "oauth_gitlab"`, `refresh_token`, `token_expires_at`, `git_base_url`

### Token refresh utility
- Function to refresh GitLab tokens when expired (using stored refresh_token)
- Called before serving git_token to workers and before provider API calls (repo list, PR creation)
- GitHub OAuth tokens don't expire in the same way — handle per provider

### Configuration
- `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `GITHUB_REDIRECT_URI`
- `GITLAB_CLIENT_ID`, `GITLAB_CLIENT_SECRET`, `GITLAB_REDIRECT_URI`, `GITLAB_BASE_URL`
- `REDIRECT_AFTER_AUTH` — URL to redirect to after successful OAuth

## Dependencies
- Task 06 (identity credentials — OAuth stores tokens on identities)

## Design decisions
- These routes are NOT protected by API key (browser redirects)
- PKCE is mandatory for both providers
- Cookie-based CSRF is simpler than DB-stored state for OAuth flows

## Test criteria
- [ ] `GET /auth/github` redirects to GitHub with correct params (client_id, code_challenge, state)
- [ ] `GET /auth/github` returns `503` when `GITHUB_CLIENT_ID` not set
- [ ] CSRF nonce mismatch on callback returns error (not silent success)
- [ ] Successful GitHub callback stores git_token on identity and redirects
- [ ] Successful GitLab callback stores git_token, refresh_token, token_expires_at, git_provider
- [ ] Token refresh works for GitLab when token is expired
- [ ] `_rh_oauth` cookie is cleared after callback
- [ ] Integration tests with mocked OAuth provider responses
- [ ] `cargo test -p server` passes
