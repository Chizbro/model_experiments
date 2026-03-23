# 23 - Web UI Settings, Auth & Bootstrap

## Goal
Build the Settings page: control plane URL configuration, API key setup (including bootstrap), BYOL credential management, and GitHub/GitLab OAuth sign-in.

## What to build

### Settings page (`web/src/pages/Settings.tsx`)

**Control plane connection**
- Input for control plane URL
- "Test connection" button: GET /health
- Status indicator: connected / unreachable / error

**API key setup**
- If connected but 401 on authenticated call:
  - Check if bootstrap is available (POST /api-keys/bootstrap probe)
  - If bootstrap available: show "Create first API key" button (one-time, with warning about security)
  - If not: show "Enter API key" input
- Input field for API key (masked)
- "Save" stores to localStorage
- Show current key status (valid/invalid)
- Create new key: POST /api-keys with optional label
- List existing keys: GET /api-keys (table with revoke buttons)

**BYOL credentials (identity management)**
- Show credential status: GET /identities/default
  - Has git token: yes/no
  - Has agent token: yes/no
- Token health: GET /identities/default/auth-status
  - Show status badges (healthy, expiring_soon, expired)
  - "Re-authenticate" button for expired tokens
- Set agent token: input field + save (PATCH /identities/default with agent_token)
- Set git token manually: input field + save (PATCH with git_token)

**Git OAuth sign-in**
- "Sign in with GitHub" button -> window.location = `{api}/auth/github?identity_id=default`
- "Sign in with GitLab" button -> window.location = `{api}/auth/gitlab?identity_id=default`
- After redirect back: check URL params for `credentials=github_ok` etc.
- Show success message and refresh credential status
- Handle 503: "OAuth not configured on this server"

**Wake URL (optional)**
- Input for wake URL (stored in localStorage)
- Used when control plane unreachable

**Log retention info**
- Display: "Default log retention: {N} days"
- Note: "Mark sessions as 'retain forever' to prevent auto-deletion"

### Bootstrap safety
- Bootstrap UI only shown after: GET /health succeeds + 401 on authenticated call + explicit probe
- Never show "create key without auth" on every visit
- After first key exists: hide bootstrap entirely

## Dependencies
- Task 22 (web UI foundation — routing, API client, layout)
- Task 05 (API key auth — bootstrap endpoint)
- Task 06 (identity credentials — endpoints)
- Task 07 (OAuth — redirect endpoints)

## Test criteria
- [ ] Control plane URL input validates with /health
- [ ] Bootstrap flow works: create first key when none exist
- [ ] Bootstrap hidden when keys already exist
- [ ] API key saved to localStorage and used in subsequent requests
- [ ] Credential status shows correctly (has_git_token, has_agent_token)
- [ ] Token health status displayed with appropriate badges
- [ ] Agent token can be set via input
- [ ] GitHub OAuth sign-in redirects to GitHub
- [ ] GitLab OAuth sign-in redirects to GitLab
- [ ] Post-OAuth redirect shows success and refreshed status
- [ ] OAuth 503 shows "not configured" message
- [ ] Wake URL stored and available when needed
