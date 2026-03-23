# 19 - PR/MR Creation

## Goal
Implement server-side Pull Request (GitHub) and Merge Request (GitLab) creation after successful task completion in PR mode. This hooks into the task complete flow from Task 10.

## What to build

### PR/MR service (`crates/server/src/services/pr_service.rs`)

**Trigger conditions** (all must hold):
- Job status is `success`
- Session `params.branch_mode` is `"pr"`
- `branch` and `mr_title` in task complete payload are non-empty
- `repo_url` recognized as GitHub or GitLab
- Identity has a valid `git_token` for the provider

**GitHub PR creation**
- `create_github_pr(repo_url, branch, base, title, description, token) -> Result<String>`
- Parse owner/repo from repo_url
- POST `https://api.github.com/repos/{owner}/{repo}/pulls`
- Body: `{ "title", "body", "head": branch, "base": "main" }`
- Return PR URL on success
- Handle errors: 401 (bad token), 422 (branch not found, PR already exists), rate limits

**GitLab MR creation**
- `create_gitlab_mr(repo_url, branch, base, title, description, token, base_url) -> Result<String>`
- Parse project path from repo_url
- POST `{base_url}/api/v4/projects/{encoded_path}/merge_requests`
- Body: `{ "title", "source_branch": branch, "target_branch": "main" }`
- Return MR URL on success
- Use identity's `git_base_url` for self-hosted GitLab

**Provider detection**
- `detect_provider(repo_url) -> Option<GitProvider>`
- Match on hostname: `github.com` -> GitHub, `gitlab.com` or `*.gitlab.com` -> GitLab
- Also check identity's `git_provider` and `git_base_url`

**Integration with task complete**
- After task complete marks job as success in PR mode:
  1. Refresh git_token if needed (GitLab OAuth expiry)
  2. Call provider API to create PR/MR
  3. Store `pull_request_url` on the job
  4. If PR creation fails: log error, update job with note (job still "completed" — PR failure is non-blocking)

**Error handling**
- PR creation failure should NOT fail the job (job already completed)
- Store error in logs: "PR/MR creation failed: {reason}"
- Surface via `error_message` or separate field so UI can explain

## Dependencies
- Task 10 (task dispatch/completion — hook into on_job_completed)
- Task 06 (identity credentials — for token resolution and refresh)
- Task 07 (OAuth — for token refresh before API call)

## Test criteria
- [ ] GitHub PR created when all conditions met (mock GitHub API)
- [ ] GitLab MR created when conditions met (mock GitLab API)
- [ ] `pull_request_url` stored on job after successful creation
- [ ] PR not created when branch_mode != "pr"
- [ ] PR not created when job status is "failed"
- [ ] PR not created when branch or mr_title is empty
- [ ] Provider correctly detected from repo_url
- [ ] Token refreshed before API call for GitLab
- [ ] PR creation failure doesn't change job status
- [ ] PR creation failure logged and surfaceable
- [ ] Self-hosted GitLab URL handled correctly
- [ ] Unit tests for provider detection
- [ ] Integration tests with mocked provider APIs
- [ ] `cargo test -p server` passes
