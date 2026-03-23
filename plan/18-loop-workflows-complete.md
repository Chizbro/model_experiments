# 18 - Loop N & Loop Until Sentinel Workflows

## Goal
Implement both loop workflow types: Loop N (fixed iterations) and Loop Until Sentinel (run until literal substring found in output).

## What to build

### Loop N — Server-side (`crates/server/src/engine/loop_n.rs`)
- On session create with `workflow: "loop_n"`: create N jobs upfront
- Each job gets task_input: `{ "prompt": "...", "iteration_index": 0..N-1 }`
- Jobs are all `pending` — workers pull them one at a time
- Session completes when all N jobs are completed
- Session fails if any job fails (remaining pending jobs stay pending or get cancelled)

### Loop Until Sentinel — Server-side (`crates/server/src/engine/loop_sentinel.rs`)
- On session create with `workflow: "loop_until_sentinel"`: create 1 initial job
- `sentinel` value stored in session params
- On job complete:
  - If `sentinel_reached: true` OR worker `output` contains `params.sentinel` (literal substring): mark session completed
  - If not reached: create next iteration job with `iteration_index` incremented
- Sentinel check: **literal substring** match, case-sensitive (per spec)
  - `output.contains(&params.sentinel)`
- No max iteration limit in v1 (user must cancel session to stop)

### Worker-side: loop task handling
- Same as any task: pull, clone, run agent, commit/push, complete
- For loop tasks: each iteration is a fresh clone (idempotent by design)
- On task complete:
  - Include `output` (agent output snippet for sentinel detection)
  - Set `sentinel_reached: true` if worker detects sentinel in output
  - Server double-checks with its own substring match on `output`

### Worker-side: sentinel detection
- After agent CLI completes, scan full output for sentinel substring
- Case-sensitive match (per v1 spec)
- Set `sentinel_reached` in TaskCompleteRequest

### Session status logic
- **Loop N**: pending -> running (when first job assigned) -> completed (all jobs done) | failed (any job failed)
- **Loop Until Sentinel**: pending -> running -> completed (sentinel found) | failed (job failed)
- Both: support `DELETE /sessions/:id` to cancel (remaining pending jobs cleaned up)

## Dependencies
- Task 09 (session/job state machine)
- Task 10 (task dispatch/completion — on_job_completed hook)
- Task 16 (worker task lifecycle)

## Test criteria
- [ ] Loop N: session create with n=3 creates 3 pending jobs
- [ ] Loop N: each job has correct iteration_index (0, 1, 2)
- [ ] Loop N: session completes when all 3 jobs complete successfully
- [ ] Loop N: session fails if any job fails
- [ ] Loop Until Sentinel: initial session creates 1 job
- [ ] Loop Until Sentinel: job complete without sentinel creates next job
- [ ] Loop Until Sentinel: job complete with sentinel marks session completed
- [ ] Sentinel detection: literal substring match works (case-sensitive)
- [ ] Sentinel detection: partial match does not trigger (e.g. sentinel "DONE" doesn't match "UNDONE" — wait, it should, it's substring)
- [ ] Worker correctly detects sentinel in agent output
- [ ] Server double-checks sentinel in output field
- [ ] Each iteration is a fresh clone
- [ ] Integration test: loop_n with n=2, verify both iterations run
- [ ] Integration test: loop_until_sentinel, verify sentinel stops the loop
- [ ] `cargo test -p server` and `cargo test -p worker` pass
