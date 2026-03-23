# 16 - Worker Task Lifecycle & Log Shipping

## Goal
Wire together the full worker task execution pipeline: pull task -> clone repo -> create branch (if PR mode) -> run agent CLI -> commit/push -> complete task. Also implement log shipping (batch POST) and local file dual-write.

## What to build

### Task executor (`crates/worker/src/task_executor.rs`)
Orchestrate the full lifecycle for a pulled task:

1. **Receive task** from poll loop (PullTaskResponse)
2. **Create work directory** (temp dir per task)
3. **Clone repo** (git_ops::clone_repo with task credentials)
4. **Checkout ref** and optionally create feature branch (if branch_mode = "pr")
5. **Branch naming**: `harness/{short_session_id}` or `{branch_name_prefix}{short_id}`
6. **Run agent CLI** (agent_executor::run_agent with prompt, persona, credentials)
7. **Stream logs** to control plane during execution (periodic batch POST)
8. **After agent completes**:
   - If agent succeeded (or per worker policy): stage, commit, push
   - Commit message: implementation-defined (e.g. from agent output or default)
   - Push to main or feature branch per branch_mode
9. **Complete task**: POST /workers/tasks/:id/complete with result
   - Include: status, branch, commit_ref, output, sentinel_reached, assistant_reply, error_message
10. **Cleanup** work directory

### Error handling at each stage
- Clone failure: complete task as failed with `[CLONE_FAILED]` + error detail
- Agent CLI missing: complete as failed with clear message
- Agent non-zero exit: complete as failed with exit code and stderr
- Push failure: complete as failed with `[PUSH_FAILED]` + auth hint
- Branch creation failure: complete as failed with `[BRANCH_FAILED]`

### Log shipping (`crates/worker/src/log_shipper.rs`)
- Buffer log entries in memory
- Flush to control plane via `POST /workers/tasks/:id/logs` every N seconds or when buffer hits size limit
- On flush failure: retry once, then continue (don't block task execution)
- Final flush on task complete (ensure all logs sent)

### Local file dual-write (`crates/worker/src/file_logger.rs`)
- Write all log entries to local files in `LOG_DIR`
- Structured JSON, one line per entry
- File per task: `{LOG_DIR}/{session_id}/{job_id}.jsonl`
- Include agent stdout/stderr, worker system messages, git operation results

### Integration with poll loop
- Update worker status to `busy` in heartbeat when executing a task
- Back to `idle` after task completes
- Report `current_job_id` in heartbeat during execution

## Dependencies
- Task 13 (worker foundation — poll loop, heartbeat)
- Task 14 (git operations — clone, commit, push)
- Task 15 (agent CLI execution — run agent)

## Test criteria
- [ ] Full lifecycle: pull task -> clone -> run agent -> commit -> push -> complete
- [ ] Task completes as "success" with branch and commit_ref when everything works
- [ ] Clone failure produces clear error in task complete
- [ ] Agent failure produces clear error with exit code
- [ ] Push failure produces clear error
- [ ] Logs streamed to control plane during execution (visible in GET /sessions/:id/logs)
- [ ] Local log files written alongside API shipping
- [ ] Worker heartbeat reports "busy" during task, "idle" after
- [ ] Work directory cleaned up after task (success and failure)
- [ ] Feature branch created in PR mode
- [ ] Commit made on correct branch
- [ ] Integration test: server + worker, create session, verify task executes end-to-end
- [ ] `cargo test -p worker` passes
