# 008: CLI — Log Tailing & Session Attach

## Goal
CLI can tail logs in real time (history + SSE stream) and attach to a live session (logs + events + input for chat). This completes the CLI as a full-featured client.

## Scope
### Commands
- `remote-harness logs tail --session-id <id> [--job-id <id>] [--level <level>] [--last N]` — Load full log history (paginate until done), print each entry, then open SSE stream and print new entries as they arrive. If `--last` is set, only load last N entries before streaming. Ctrl+C to stop.
- `remote-harness logs delete --session-id <id> [--job-id <id>]` — DELETE /sessions/:id/logs. Confirm before deleting (unless --yes flag).
- `remote-harness attach <session_id>` — Combined view: show session status, load + stream logs, show session events (started, job_started, completed, etc.). For chat sessions: accept user input (stdin) and send via POST /sessions/:id/input.

### SSE client
- `sse.rs` — SSE client using reqwest streaming response. Parse `event:` and `data:` lines from text/event-stream. Reconnect with exponential backoff on disconnect (up to 30s). Stop when session is terminal (completed/failed) per session event.

### Log formatting
- Each log line: `[TIMESTAMP] [LEVEL] [SOURCE] MESSAGE`
- Color-code by level (error=red, warn=yellow, info=default, debug=dim) using terminal ANSI codes.
- Session events printed as `>>> Session [EVENT] (job: JOB_ID)` in bold.

### Attach behavior
- Load session status first (GET /sessions/:id)
- If session is terminal, just show logs (no streaming, no input)
- If session is active: load logs + start streaming + start event stream
- For chat workflow: after each job completes, prompt user for input. Send via POST /sessions/:id/input.
- Print "Reconnecting..." on SSE disconnect

## Prerequisites
- Spec 007 (CLI core)
- Spec 005 (log streaming + events SSE on server)

## Files to create/modify
- `crates/cli/src/commands/logs.rs` — New: tail, delete commands
- `crates/cli/src/commands/attach.rs` — New: attach command
- `crates/cli/src/sse.rs` — New: SSE client
- `crates/cli/src/commands/mod.rs` — Mount new commands
- `crates/cli/src/main.rs` — Add subcommands

## Acceptance criteria
1. `logs tail --session-id <id>` → prints full history then streams new entries
2. `logs tail --session-id <id> --last 10` → prints last 10 entries then streams
3. `logs tail --session-id <id> --level error` → only error-level entries
4. Log entries are formatted with timestamp, level, source
5. `logs delete --session-id <id>` → confirms then deletes
6. `attach <session_id>` → shows status + logs + events
7. For active chat session: attach prompts for input after job completes
8. SSE reconnects on disconnect with backoff
9. Attach exits when session reaches terminal state
10. Ctrl+C cleanly exits streaming commands
11. `cargo build -p cli` succeeds
12. `cargo clippy -p cli -- -D warnings` clean

## Implementation notes
- SSE parsing: read response as byte stream, split on `\n\n`, parse each event block for `event:` and `data:` fields. Use reqwest's `.bytes_stream()`.
- For reconnection: track last log timestamp; on reconnect, could re-fetch only newer logs (or just reconnect SSE and accept some duplicate filtering client-side).
- Attach is essentially logs tail + events stream + optional input loop, composed together.
- Use `tokio::select!` to multiplex SSE streams with stdin for interactive attach.
