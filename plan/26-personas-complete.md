# 26 - Personas (Server + CLI + Web)

## Goal
Implement persona CRUD across all layers: server API, CLI commands, and Web UI. Personas are user-defined prompts that give agents consistent identities.

## What to build

### Server routes (`crates/server/src/routes/personas.rs`)

**POST /personas**
- Create persona with name and prompt
- Response: `201 { persona_id, name, prompt }`

**GET /personas**
- Paginated list (name only, prompt omitted for brevity)
- Response: `200 { items: [{ persona_id, name }], next_cursor }`

**GET /personas/:id**
- Full detail including prompt
- Response: `200 { persona_id, name, prompt }`
- `404` if not found

**PATCH /personas/:id**
- Partial update of name and/or prompt
- Response: `204`

**DELETE /personas/:id**
- Response: `204`, `404` if not found

### Persona resolution in task dispatch
- When a session has `persona_id`:
  - Look up persona from DB
  - Set `prompt_context` in pull task response to persona's prompt text
- When no persona: `prompt_context` is empty/omitted

### CLI commands
- `remote-harness persona create --name "Refactorer" --prompt "You are a code refactoring expert..."`
- `remote-harness persona list`
- `remote-harness persona show <id>`
- `remote-harness persona delete <id>`
- Add `--persona-id` flag to `session start`

### Web UI
- Persona management in Settings (or separate Personas page)
  - List personas (table: name | actions)
  - Create persona (name + prompt textarea)
  - Edit persona (inline or dialog)
  - Delete persona (with confirmation)
- Persona dropdown in session creation form

## Dependencies
- Task 04 (server foundation)
- Task 10 (task dispatch — persona resolution in pull response)
- Task 20/21 (CLI — add persona commands)
- Task 22/23 (Web UI — add to settings or separate page)

## Test criteria
- [ ] POST /personas creates persona
- [ ] GET /personas lists all personas
- [ ] GET /personas/:id returns full detail with prompt
- [ ] PATCH /personas/:id updates name/prompt
- [ ] DELETE /personas/:id removes persona
- [ ] Session with persona_id includes prompt_context in pulled task
- [ ] Session without persona_id has empty prompt_context
- [ ] CLI persona commands work end-to-end
- [ ] Web UI persona management works (create, list, edit, delete)
- [ ] Persona dropdown appears in session creation form
- [ ] `cargo test -p server` passes
