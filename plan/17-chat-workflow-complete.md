# 17 - Chat Workflow

## Goal
Implement the chat workflow end-to-end: single-turn and multi-turn conversations with history assembly, history cap, and proper task_input construction.

## What to build

### Server-side: chat job creation
- **First job** (session create): task_input = `{ "prompt": session.params.prompt }`
- **Follow-up job** (POST /sessions/:id/input): task_input = `{ "session_prompt", "message", "history", "history_assistant", "history_truncated" }`
  - `session_prompt`: original prompt from session create
  - `message`: the current follow-up message
  - `history`: list of prior user follow-up messages (from previous inputs)
  - `history_assistant`: list of prior assistant replies (from task complete `assistant_reply` fields)
  - `history_truncated`: true if any turns were dropped due to cap

### Server-side: history assembly (`crates/server/src/engine/chat.rs`)
- Query all completed jobs for this session, ordered by creation
- Extract user messages and assistant replies
- Apply cap: keep last `CHAT_HISTORY_MAX_TURNS` entries per side
- Set `history_truncated = true` when cap causes drops
- Build TaskInput and store on the new job

### Server-side: session state for chat
- Session stays "running" until explicitly ended or no more input expected
- New input creates a new job; session goes back to "pending" if worker isn't working on it
- Session "completed" only when... (chat is open-ended; may need explicit close or timeout)
  - For v1: session stays running, user can keep sending input

### Worker-side: chat task handling
- For first job: pass prompt directly to agent CLI
- For follow-up: construct a combined prompt that includes session_prompt, history context, and current message
- Pass persona prompt_context as system context
- Capture assistant_reply from agent output
- Include assistant_reply in task complete

### Worker-side: prompt construction for multi-turn
- Build a single prompt that represents the conversation:
  ```
  [Persona context if set]
  Original goal: {session_prompt}

  Conversation history:
  User: {history[0]}
  Assistant: {history_assistant[0]}
  ...

  User: {message}
  ```
- Or use Claude Code's built-in conversation support if available

## Dependencies
- Task 09 (session/job state machine — job creation)
- Task 10 (task dispatch — pull/complete)
- Task 16 (worker task lifecycle — full execution pipeline)

## Test criteria
- [ ] Create chat session: first job has correct task_input with prompt
- [ ] Send input: follow-up job has session_prompt, message, history, history_assistant
- [ ] History correctly assembled from prior completed jobs
- [ ] History cap applies at configured limit (default 50)
- [ ] `history_truncated` is true when cap exceeded, false otherwise
- [ ] Worker passes prompt correctly to agent CLI
- [ ] Worker captures and returns assistant_reply
- [ ] Multi-turn conversation works: create session -> complete first job -> send input -> complete second job -> verify history grows
- [ ] Session status transitions correctly through chat lifecycle
- [ ] Integration test: full multi-turn chat workflow
- [ ] `cargo test -p server` and `cargo test -p worker` pass
