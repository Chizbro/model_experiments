# 011: Chat Multi-Turn (Follow-Up Messages & History)

## Goal
Chat sessions support multi-turn conversation: users send follow-up messages, the server creates new jobs with conversation history, and the worker sees the full context. History is capped per config.

## Scope
### Server
- `POST /sessions/:id/input` — Accept follow-up message for chat sessions. Validate: session exists, workflow=chat, session status is running or pending (after first job completes). Create a new job with task_input containing session_prompt, message, history, history_assistant, history_truncated.
- History construction: query all completed jobs for the session, ordered by created_at. Extract user messages (from job.task_input.message or task_input.prompt for first job) into `history` array. Extract assistant replies (from job.assistant_reply) into `history_assistant` array. Cap at CHAT_HISTORY_MAX_TURNS (default 50) per side; if capped, set `history_truncated: true`.
- `task_complete` with `assistant_reply`: store on job row so it's available for history construction.

### Worker
- When executing a chat follow-up job (task_input has history): construct prompt that includes session_prompt, history, and current message. Pass the combined context to the agent CLI.
- Capture assistant reply from agent output and include in task_complete.

### CLI
- `attach` command: for chat sessions, after each job completes, prompt for input. Send via POST /sessions/:id/input. Show new job's logs.

### Web UI
- Session detail: for chat sessions, show input box. After sending, create new job visible in jobs list. Stream new job's logs.

## Prerequisites
- Spec 004 (task completion)
- Spec 006 (worker execution)
- Spec 007-008 (CLI attach)
- Spec 010 (Web UI session detail)

## Files to create/modify
- `crates/server/src/routes/sessions.rs` — Add `send_input` handler
- `crates/server/src/engine/mod.rs` — History construction logic, follow-up job creation
- `crates/worker/src/task_loop.rs` — Handle multi-turn task_input
- `crates/worker/src/agent_runner.rs` — Construct combined prompt with history
- `crates/cli/src/commands/attach.rs` — Interactive input for chat
- `web/src/pages/SessionDetail.tsx` — Chat input handling

## Acceptance criteria
1. `POST /sessions/:id/input` with message → 202, new job created
2. New job's task_input includes session_prompt, message, history, history_assistant
3. History is capped at CHAT_HISTORY_MAX_TURNS; history_truncated set when capped
4. `POST /sessions/:id/input` on non-chat session → 409
5. `POST /sessions/:id/input` on completed session → 409
6. Worker receives multi-turn task_input and constructs appropriate prompt
7. Worker captures assistant_reply and includes in task_complete
8. CLI attach: can send follow-up messages interactively
9. Web UI: can send follow-up messages, see new job logs
10. History construction preserves chronological order
11. `cargo test` — at least 3 tests (history construction, capping, input validation)

## Implementation notes
- History cap: take last N entries from each array. If original length > N, set truncated=true.
- The combined prompt for the agent CLI could be: system prompt (persona), then `session_prompt` as context, then history interleaved, then current message.
- CLI attach: use `tokio::io::BufReader` on stdin to read lines. Each line is a follow-up message.
