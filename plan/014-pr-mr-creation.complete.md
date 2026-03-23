# 014: PR/MR Creation (Server-Side)

## Goal
When a job completes successfully in PR mode (branch_mode=pr), the server creates a Pull Request (GitHub) or Merge Request (GitLab) via the provider API.

## Scope
### Server logic (in task_complete handler)
After a job is marked completed with status=success:
1. Check session params: branch_mode must be "pr"
2. Check job: branch and mr_title must be non-empty
3. Detect provider from repo_url (github.com → GitHub, gitlab.com or git_base_url → GitLab)
4. Get git_token from identity (refresh if needed)
5. Call provider API to create PR/MR:
   - GitHub: POST /repos/{owner}/{repo}/pulls with title, body, head (branch), base (ref or main)
   - GitLab: POST /projects/{id}/merge_requests with title, description, source_branch, target_branch
6. Store pull_request_url on the job
7. If PR/MR creation fails: log error, do NOT fail the job (job already completed). Store error in logs.

### Worker enhancements
- When branch_mode=pr: after agent run, generate mr_title and mr_description from agent output (use last line or a simple heuristic, or a short model call if configured). Include in task_complete payload.
- Ensure branch name is set (e.g., `harness/{short_session_id}` or prefix from params).

### API/UI visibility
- Job detail shows pull_request_url as a clickable link
- If PR expected but null: show explanation per CLIENT_EXPERIENCE.md §8

## Prerequisites
- Spec 004 (task completion)
- Spec 006 (worker, git push)
- Spec 013 (token refresh for git_token)

## Files to create/modify
- `crates/server/src/engine/mod.rs` — PR/MR creation logic after task_complete
- `crates/server/src/engine/pr.rs` — New: GitHub and GitLab PR/MR API calls
- `crates/worker/src/task_loop.rs` — Set mr_title/mr_description in complete payload
- `crates/worker/src/git_ops.rs` — Ensure branch name generation
- `web/src/pages/SessionDetail.tsx` — Show PR URL or explanation

## Acceptance criteria
1. Job completes in PR mode with branch → PR created on GitHub
2. pull_request_url stored on job and visible in API/UI
3. If PR creation fails: job still completed, error logged
4. If branch_mode is not "pr": no PR attempted
5. If branch or mr_title is empty: no PR attempted, logged
6. GitLab MR creation works with GitLab API
7. Web UI shows PR URL as link, or explains why missing
8. `cargo test` — at least 2 tests (PR creation logic, skip conditions)

## Implementation notes
- Parse repo owner/name from repo_url: e.g., `https://github.com/owner/repo.git` → owner=owner, repo=repo
- GitHub API: POST https://api.github.com/repos/{owner}/{repo}/pulls, headers: Authorization: Bearer {token}, Accept: application/json
- GitLab API: need project ID. Get via GET /projects/{url_encoded_path}. Then POST /projects/{id}/merge_requests.
- PR/MR creation is best-effort and non-blocking for the job completion flow.
