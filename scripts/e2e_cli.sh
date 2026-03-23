#!/usr/bin/env bash
# End-to-end exercise of `remote-harness` CLI against a running control plane.
#
# Prereq: server reachable (default http://127.0.0.1:3000), DB migrated, ready OK.
#
# API key resolution:
#   1) If REMOTE_HARNESS_API_KEY or API_KEY works for `api-key list`, use it.
#   2) Else try `api-key bootstrap` (only when the server has no API_KEY/API_KEYS env
#      and no keys in DB — see crates/server/src/keys.rs).
#   3) If bootstrap is forbidden, set REMOTE_HARNESS_API_KEY to match the server's API_KEY.
#
# Session defaults (override with env):
#   E2E_REPO_URL     — HTTPS git URL (required by server; must be non-empty)
#   E2E_AGENT_CLI    — cursor | claude_code
#   E2E_PROMPT       — non-empty prompt for workflow chat
#   E2E_IDENTITY_ID  — default | <uuid> (passed as --identity-id when not "default")
#
# Control plane URL: REMOTE_HARNESS_URL or CONTROL_PLANE_URL (CLI precedence); this script
# passes --control-plane-url explicitly from REMOTE_HARNESS_URL.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

REMOTE_HARNESS_URL="${REMOTE_HARNESS_URL:-${CONTROL_PLANE_URL:-http://127.0.0.1:3000}}"
export REMOTE_HARNESS_URL

E2E_REPO_URL="${E2E_REPO_URL:-https://github.com/octocat/Hello-World.git}"
E2E_AGENT_CLI="${E2E_AGENT_CLI:-cursor}"
E2E_PROMPT="${E2E_PROMPT:-e2e cli smoke}"
E2E_IDENTITY_ID="${E2E_IDENTITY_ID:-default}"

RH="$ROOT/target/debug/remote-harness"
FAILED=0

say() { printf '\n=== %s ===\n' "$*"; }

die() {
  echo "e2e_cli: $*" >&2
  exit 1
}

bump_fail() {
  echo "e2e_cli: FAIL — $*" >&2
  FAILED=$((FAILED + 1))
}

expect_ok() {
  local label="$1"
  shift
  local out
  if ! out="$("$@" 2>&1)"; then
    bump_fail "$label — exit nonzero — $* — $out"
    return 1
  fi
  printf '%s\n' "$out"
  return 0
}

expect_ok_stdout_matches() {
  local label="$1"
  local pattern="$2"
  shift 2
  local out
  if ! out="$("$@" 2>&1)"; then
    bump_fail "$label — exit nonzero — $* — $out"
    return 1
  fi
  if ! printf '%s\n' "$out" | grep -Fq "$pattern"; then
    bump_fail "$label — stdout missing pattern $(printf %q "$pattern") — $* — $out"
    return 1
  fi
  printf '%s\n' "$out"
  return 0
}

expect_ok_stdout_regex() {
  local label="$1"
  local regex="$2"
  shift 2
  local out
  if ! out="$("$@" 2>&1)"; then
    bump_fail "$label — exit nonzero — $* — $out"
    return 1
  fi
  if ! printf '%s\n' "$out" | grep -Eq "$regex"; then
    bump_fail "$label — stdout did not match regex $(printf %q "$regex") — $* — $out"
    return 1
  fi
  printf '%s\n' "$out"
  return 0
}

rh() {
  "$RH" --control-plane-url "$REMOTE_HARNESS_URL" "$@"
}

rh_key() {
  "$RH" --control-plane-url "$REMOTE_HARNESS_URL" --remote-harness-api-key "$E2E_API_KEY" "$@"
}

# Run a long-running SSE command; SIGTERM after idle_secs; treat 0 / 128+n as OK if output matches.
sse_probe() {
  local label="$1"
  local idle_secs="$2"
  local pattern="$3"
  shift 3
  local outf
  outf="$(mktemp)"
  "$@" >"$outf" 2>&1 &
  local cpid=$!
  (sleep "$idle_secs"; kill -TERM "$cpid" 2>/dev/null) &
  wait "$cpid" || true
  local body
  body="$(cat "$outf")"
  rm -f "$outf"
  if ! printf '%s\n' "$body" | grep -Fq "$pattern"; then
    bump_fail "$label — SSE output missing $(printf %q "$pattern") — $* — $body"
    return 1
  fi
  printf '%s\n' "$body"
  return 0
}

parse_bootstrap_key() {
  awk '
    /^API key \(store this; shown once\):$/ { want=1; next }
    want { print; exit }
  '
}

extract_session_id() {
  awk -F': ' '/^session_id: / { gsub(/\r/, "", $2); print $2; exit }'
}

extract_first_job_id() {
  awk '/^  job / { print $2; exit }'
}

extract_task_id_from_pull() {
  local text="$1"
  if [[ "$text" == *"no task (204)"* ]]; then
    echo ""
    return 0
  fi
  if command -v jq >/dev/null 2>&1; then
    echo "$text" | jq -r '.task_id // empty' 2>/dev/null || true
    return 0
  fi
  echo "$text" | sed -n 's/.*"task_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1
}

extract_api_key_id_from_create() {
  awk '/^id:/{ sub(/^id:[[:space:]]+/, ""); print; exit }'
}

ensure_cli_built() {
  if [[ ! -x "$RH" ]]; then
    say "build cli"
    cargo build -p cli -q
  fi
  [[ -x "$RH" ]] || die "missing $RH (cargo build -p cli)"
}

api_key_list_ok() {
  local key="$1"
  E2E_API_KEY="$key" rh_key api-key list >/dev/null 2>&1
}

ensure_api_key() {
  local try_key="${REMOTE_HARNESS_API_KEY:-}"
  if [[ -z "$try_key" ]]; then
    try_key="${API_KEY:-}"
  fi

  if [[ -n "$try_key" ]] && api_key_list_ok "$try_key"; then
    E2E_API_KEY="$try_key"
    export REMOTE_HARNESS_API_KEY="$E2E_API_KEY"
    echo "e2e_cli: using existing API key from env (REMOTE_HARNESS_API_KEY or API_KEY)."
    return 0
  fi

  say "api-key bootstrap (isolated HOME — no config file)"
  local boot_home
  boot_home="$(mktemp -d "${TMPDIR:-/tmp}/rh-e2e-home.XXXXXX")"
  local boot_out
  if ! boot_out="$(
    env -i \
      HOME="$boot_home" \
      PATH="$PATH" \
      REMOTE_HARNESS_URL="$REMOTE_HARNESS_URL" \
      "$RH" --control-plane-url "$REMOTE_HARNESS_URL" api-key bootstrap --label e2e-cli-bootstrap 2>&1
  )"; then
    rm -rf "$boot_home"
    echo "$boot_out" >&2
    die "bootstrap failed. If the server sets API_KEY/API_KEYS or keys already exist, export REMOTE_HARNESS_API_KEY to a valid key."
  fi
  rm -rf "$boot_home"

  local new_key
  new_key="$(printf '%s\n' "$boot_out" | parse_bootstrap_key)"
  [[ -n "$new_key" ]] || die "could not parse key from bootstrap stdout"

  E2E_API_KEY="$new_key"
  export REMOTE_HARNESS_API_KEY="$E2E_API_KEY"
  echo "e2e_cli: bootstrapped new API key (exported REMOTE_HARNESS_API_KEY for this shell)."
}

identity_args() {
  if [[ "$E2E_IDENTITY_ID" == "default" ]]; then
    echo ""
  else
    echo "--identity-id" "$E2E_IDENTITY_ID"
  fi
}

main() {
  ensure_cli_built

  say "version marker"
  expect_ok_stdout_regex "version marker" '^api-types ' rh --version-marker

  say "default help (no subcommand)"
  expect_ok_stdout_matches "help banner" "remote-harness CLI" rh

  say "health / ready (unauthenticated)"
  expect_ok_stdout_matches "health" "control plane healthy (" rh health
  expect_ok_stdout_matches "ready" "control plane ready (" rh ready

  say "oauth URL helpers (no network)"
  expect_ok_stdout_regex "oauth github" '/auth/github\?' rh oauth github
  expect_ok_stdout_regex "oauth gitlab" '/auth/gitlab\?' rh oauth gitlab

  say "resolve API key"
  ensure_api_key

  say "config show"
  expect_ok_stdout_matches "config show" "control_plane_url:" rh_key config show

  say "idle (informational — may fail when jobs exist)"
  if ! out="$(rh_key idle 2>&1)"; then
    echo "$out"
    if printf '%s\n' "$out" | grep -Fq "not idle"; then
      echo "e2e_cli: idle: skipped (control plane busy) — not a failure."
    else
      bump_fail "idle — unexpected error — $out"
    fi
  else
    printf '%s\n' "$out"
    if ! printf '%s\n' "$out" | grep -Fq "control plane idle"; then
      bump_fail "idle — expected idle message — $out"
    fi
  fi

  say "api-key list"
  expect_ok "api-key list" rh_key api-key list

  say "api-key create + delete"
  local create_out key_id
  create_out="$(expect_ok "api-key create" rh_key api-key create --label "e2e-cli-$(date +%s)")"
  key_id="$(printf '%s\n' "$create_out" | extract_api_key_id_from_create)"
  [[ -n "$key_id" ]] || die "could not parse new api key id"
  expect_ok_stdout_matches "api-key delete" "revoked API key" rh_key api-key delete "$key_id"

  say "identity + credentials"
  expect_ok_stdout_matches "identity get" "has_git_token:" rh_key identity get "$E2E_IDENTITY_ID"
  expect_ok_stdout_matches "identity auth-status" "git_token_status:" rh_key identity auth-status "$E2E_IDENTITY_ID"
  local repos_out
  if repos_out="$(rh_key identity repos "$E2E_IDENTITY_ID" 2>&1)"; then
    printf '%s\n' "$repos_out"
    if ! printf '%s\n' "$repos_out" | grep -Fq "provider:"; then
      bump_fail "identity repos — expected provider: line — $repos_out"
    fi
  else
    printf '%s\n' "$repos_out"
    if printf '%s\n' "$repos_out" | grep -Fq "Git token is not configured"; then
      echo "e2e_cli: identity repos skipped (no git token on identity)."
    else
      bump_fail "identity repos — $repos_out"
    fi
  fi
  expect_ok_stdout_matches "credentials show" "has_git_token:" rh_key credentials show "$E2E_IDENTITY_ID"

  local wid="e2e-cli-worker-$$-${RANDOM}"
  say "worker register / get / heartbeat / pull"
  expect_ok_stdout_matches "worker register" "registered worker_id:" rh_key worker register "$wid" --host e2e-cli
  expect_ok_stdout_matches "worker get" "worker_id:" rh_key worker get "$wid"
  expect_ok_stdout_matches "worker heartbeat" "heartbeat ok" rh_key worker heartbeat "$wid"

  say "session create / list / get / patch / patch-job"
  local session_out session_id job_id
  # shellcheck disable=SC2046
  session_out="$(
    expect_ok "session create" rh_key session create "$E2E_REPO_URL" \
      --workflow chat \
      --prompt "$E2E_PROMPT" \
      --agent-cli "$E2E_AGENT_CLI" \
      $(identity_args)
  )"
  session_id="$(printf '%s\n' "$session_out" | extract_session_id)"
  [[ -n "$session_id" ]] || die "could not parse session_id"

  expect_ok_stdout_matches "session list" "$session_id" rh_key session list --limit 5

  local get_out
  get_out="$(expect_ok "session get" rh_key session get "$session_id")"
  job_id="$(printf '%s\n' "$get_out" | extract_first_job_id)"
  if ! printf '%s\n' "$get_out" | grep -Fq "$session_id"; then
    bump_fail "session get — output missing session id $session_id — $get_out"
  fi

  expect_ok_stdout_matches "session patch" "retain_forever updated" rh_key session patch "$session_id" --retain-forever true
  if [[ -n "$job_id" ]]; then
    expect_ok_stdout_matches "session patch-job" "retain_forever updated" \
      rh_key session patch-job "$session_id" "$job_id" --retain-forever false
  else
    echo "e2e_cli: no job id on session get — skipping session patch-job"
  fi

  say "worker pull → optional logs send + complete"
  local pull_out task_id
  pull_out="$(expect_ok "worker pull" rh_key worker pull --worker-id "$wid")"
  task_id="$(extract_task_id_from_pull "$pull_out")"
  if [[ -n "$task_id" ]]; then
    local log_json='[{"timestamp":"2025-01-01T00:00:00Z","level":"info","message":"e2e-cli","source":"worker"}]'
    expect_ok_stdout_matches "logs send" "accepted:" rh_key logs send "$task_id" --json "$log_json"
    expect_ok_stdout_matches "worker complete" "task complete ok" \
      rh_key worker complete "$task_id" --worker-id "$wid" --status success
  else
    echo "e2e_cli: pull returned no task — skipping logs send / worker complete (assign a job or retry)."
  fi

  say "logs list"
  expect_ok "logs list" rh_key logs list "$session_id" --limit 5

  say "logs tail (SSE, SIGTERM after 4s)"
  sse_probe "logs tail" 4 "--- streaming (GET .../logs/stream)" \
    rh_key logs tail "$session_id" --last 3

  say "attach --follow-logs (SSE, SIGTERM after 6s)"
  sse_probe "attach" 6 "] " rh_key attach "$session_id" --follow-logs

  say "logs delete (session)"
  expect_ok_stdout_matches "logs delete" "logs deleted" rh_key logs delete "$session_id"

  say "session delete"
  expect_ok_stdout_matches "session delete" "deleted" rh_key session delete "$session_id"

  say "worker delete"
  expect_ok_stdout_matches "worker delete" "deleted worker" rh_key worker delete "$wid"

  if [[ "$FAILED" -ne 0 ]]; then
    die "$FAILED assertion(s) failed."
  fi
  say "e2e_cli: OK"
}

main "$@"
