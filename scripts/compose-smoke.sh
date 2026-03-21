#!/usr/bin/env bash
# End-to-end smoke: Compose stack + chat session to completed + logs via API.
#
# Tier 1 (default, CI-friendly): shared API_KEY from compose default, REMOTE_HARNESS_STUB_AGENT,
#   file:// bare repo mounted into the worker at /e2e/repo.git.
# Tier 2 (local / real agent): omit this overlay; set identity tokens and use a real HTTPS remote;
#   do not set REMOTE_HARNESS_STUB_AGENT on the worker.
#
# Optional: RH_SMOKE_BOOTSTRAP=1 — start postgres+server+web first with no server API_KEY, call
#   POST /api-keys/bootstrap, then start the worker with the new key (simulates first deploy).
#
# API calls use `docker compose exec server curl` to localhost:3000 so the script works even when
# host port publishing to 127.0.0.1 is flaky (some Docker setups).
#
# Requires: docker compose, git, python3 (for JSON parsing). The server image includes curl.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

COMPOSE_BASE=(docker compose -f docker-compose.yml -f docker-compose.smoke.yml)
COMPOSE_ALL=("${COMPOSE_BASE[@]}") # may switch to bootstrap merge below

API_INTERNAL="http://127.0.0.1:3000"

TMP=""
shutdown_stack() {
  if [[ "${RH_SMOKE_KEEP_STACK:-}" == "1" ]]; then
    return 0
  fi
  "${COMPOSE_ALL[@]}" down --remove-orphans 2>/dev/null || true
}

cleanup() {
  shutdown_stack
  if [[ -n "${TMP}" && -d "${TMP}" ]]; then
    rm -rf "${TMP}"
  fi
}
trap cleanup EXIT

# Keep the bare repo inside the workspace so Docker Desktop bind mounts work (often excludes /var/folders and sometimes /tmp).
mkdir -p "${ROOT}/.compose-smoke-fixture"
TMP="$(mktemp -d "${ROOT}/.compose-smoke-fixture/run.XXXXXX")"
BARE="${TMP}/origin.git"
git init --bare "${BARE}"
WORKDIR="${TMP}/wt"
git clone "${BARE}" "${WORKDIR}"
git -C "${WORKDIR}" config user.email "smoke@remote-harness.local"
git -C "${WORKDIR}" config user.name "smoke"
echo "e2e" >"${WORKDIR}/README.md"
git -C "${WORKDIR}" add README.md
git -C "${WORKDIR}" commit -m "init"
git -C "${WORKDIR}" branch -M main
git -C "${WORKDIR}" push -u origin main

export RH_E2E_REPO_BARE_PATH="${BARE}"
REPO_URL="file:///e2e/repo.git"

# Invoke curl inside the server container (same network as DB; avoids host port mapping issues).
rh_curl() {
  "${COMPOSE_ALL[@]}" exec -T server curl -fsS "$@"
}

wait_server_health() {
  local label="$1" max="${2:-200}"
  local i=0
  while (( i < max )); do
    if rh_curl "${API_INTERNAL}/health" >/dev/null 2>&1; then
      echo "${label}: OK (${API_INTERNAL} from server container)"
      return 0
    fi
    sleep 1
    ((i++)) || true
  done
  echo "${label}: timeout waiting for server /health" >&2
  echo "--- docker compose logs (server) ---" >&2
  "${COMPOSE_ALL[@]}" logs --no-color --tail=80 server 2>&1 || true
  return 1
}

# Bust Docker layer cache for Rust images so `RUN cargo build` cannot reuse a stale binary.
export RH_DOCKER_SRC_TS="${RH_DOCKER_SRC_TS:-$(date +%s)}"

if [[ "${RH_SMOKE_BOOTSTRAP:-}" == "1" ]]; then
  COMPOSE_ALL=(docker compose -f docker-compose.yml -f docker-compose.smoke.yml -f docker-compose.smoke-bootstrap.yml)
  echo "Smoke (bootstrap path): starting postgres, server, web (no worker yet)…"
  "${COMPOSE_ALL[@]}" up -d --build postgres server web
  wait_server_health "control plane"

  BOOT_JSON="$(rh_curl -X POST "${API_INTERNAL}/api-keys/bootstrap" \
    -H "Content-Type: application/json" \
    -d '{"label":"compose-smoke-bootstrap"}')"
  API_KEY="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["key"])' <<<"${BOOT_JSON}")"
  export API_KEY
  echo "Bootstrapped API key (saved in shell only for this run)."
  echo "Starting worker…"
  "${COMPOSE_ALL[@]}" up -d --build worker
else
  API_KEY="${API_KEY:-${REMOTE_HARNESS_API_KEY:-dev-key-change-in-production}}"
  export API_KEY
  echo "Smoke (shared key path): API_KEY from env or compose default."
  "${COMPOSE_ALL[@]}" up -d --build
  wait_server_health "control plane"
fi

if "${COMPOSE_ALL[@]}" exec -T web wget -qO- http://127.0.0.1/health >/dev/null 2>&1; then
  echo "web static: OK (in-container /health)"
else
  echo "web static: skip or retry from host (in-container check failed)" >&2
fi

rh_curl -X PATCH "${API_INTERNAL}/identities/default" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "Content-Type: application/json" \
  -d '{"agent_token":"e2e-compose-smoke","git_token":"e2e-compose-smoke"}' >/dev/null

CREATE_BODY="$(cat <<EOF
{"repo_url":"${REPO_URL}","ref":"main","workflow":"chat","params":{"prompt":"smoke hello","agent_cli":"cursor"}}
EOF
)"
CREATE_RESP="$(rh_curl -X POST "${API_INTERNAL}/sessions" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "Content-Type: application/json" \
  -d "${CREATE_BODY}")"
SID="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["session_id"])' <<<"${CREATE_RESP}")"
echo "session_id=${SID}"

# Chat workflow: session stays `running` after the job completes so clients can POST follow-up input
# (see `crates/server/src/worker_tasks.rs`: session_status when workflow == "chat").
# Non-chat workflows set session status to `completed` when the job finishes.
deadline=$((SECONDS + 300))
status=""
smoke_ok=""
while (( SECONDS < deadline )); do
  DETAIL="$(rh_curl "${API_INTERNAL}/sessions/${SID}" -H "Authorization: Bearer ${API_KEY}")"
  status="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["status"])' <<<"${DETAIL}")"
  smoke_ok="$(python3 -c '
import json, sys
d = json.load(sys.stdin)
st = d.get("status") or ""
if st == "failed":
    print("failed")
elif st == "completed":
    print("yes")
else:
    jobs = d.get("jobs") or []
    if d.get("workflow") == "chat" and len(jobs) == 1 and jobs[0].get("status") == "completed":
        print("yes")
    else:
        print("no")
' <<<"${DETAIL}")"
  if [[ "${smoke_ok}" == "yes" ]]; then
    if [[ "${status}" == "completed" ]]; then
      echo "session status: completed"
    else
      echo "session status: ${status} (chat job completed — session may stay running for follow-up)"
    fi
    break
  fi
  if [[ "${smoke_ok}" == "failed" ]]; then
    echo "session failed:" >&2
    echo "${DETAIL}" | python3 -m json.tool >&2 || echo "${DETAIL}" >&2
    exit 1
  fi
  sleep 2
done

if [[ "${smoke_ok}" != "yes" ]]; then
  echo "timeout waiting for terminal success (last session status=${status})" >&2
  echo "--- docker compose logs (worker) ---" >&2
  "${COMPOSE_ALL[@]}" logs --no-color --tail=120 worker 2>&1 || true
  echo "--- docker compose logs (server) ---" >&2
  "${COMPOSE_ALL[@]}" logs --no-color --tail=80 server 2>&1 || true
  exit 1
fi

LOGS_JSON="$(rh_curl "${API_INTERNAL}/sessions/${SID}/logs?limit=50" \
  -H "Authorization: Bearer ${API_KEY}")"
LOG_COUNT="$(python3 -c 'import json,sys; print(len(json.load(sys.stdin).get("items") or []))' <<<"${LOGS_JSON}")"
if [[ "${LOG_COUNT}" -lt 1 ]]; then
  echo "expected at least one log line via API, got ${LOG_COUNT}" >&2
  echo "${LOGS_JSON}" >&2
  exit 1
fi
echo "logs via API: ${LOG_COUNT} line(s) in first page — OK"

echo "compose-smoke: OK"
