import { useEffect, useState, type CSSProperties } from "react";
import { Link } from "react-router-dom";
import { useSettings } from "../hooks/useSettings";
import { readStoredGitIdentityId, writeStoredGitIdentityId } from "../settings/storage";

type ProbeState = "pending" | "ok" | "error";

interface ProbeRow {
  label: string;
  path: string;
  state: ProbeState;
  detail: string;
}

/** Keep in sync with workspace `[workspace.package] version` / api-types release (worker register semver gate). */
const CONTROL_PLANE_RELEASE = "0.1.0";

export function PlaygroundPage() {
  const { controlPlaneUrl, apiKey } = useSettings();
  const base = controlPlaneUrl ?? "";

  const [rows, setRows] = useState<ProbeRow[]>([
    { label: "GET /health", path: "/health", state: "pending", detail: "…" },
    { label: "GET /ready", path: "/ready", state: "pending", detail: "…" },
    { label: "GET /health/idle", path: "/health/idle", state: "pending", detail: "…" },
  ]);
  const [identityId, setIdentityId] = useState(() => readStoredGitIdentityId());
  const [repoProvider, setRepoProvider] = useState("github");
  const [identityGetOut, setIdentityGetOut] = useState("");
  const [identityAuthOut, setIdentityAuthOut] = useState("");
  const [identityReposOut, setIdentityReposOut] = useState("");
  const [identityPatchAgent, setIdentityPatchAgent] = useState("");
  const [identityPatchGit, setIdentityPatchGit] = useState("");
  const [identityPatchOut, setIdentityPatchOut] = useState("");
  const [workersOut, setWorkersOut] = useState("");
  const [workerRegId, setWorkerRegId] = useState("");
  const [workerRegVersion, setWorkerRegVersion] = useState(CONTROL_PLANE_RELEASE);
  const [workerPullId, setWorkerPullId] = useState("");
  const [workerPullOut, setWorkerPullOut] = useState("");
  const [workerCompleteTaskId, setWorkerCompleteTaskId] = useState("");
  const [workerCompleteWorkerId, setWorkerCompleteWorkerId] = useState("");
  const [workerCompleteSentinelReached, setWorkerCompleteSentinelReached] = useState(false);
  const [workerCompleteOut, setWorkerCompleteOut] = useState("");
  const [sessionRepoUrl, setSessionRepoUrl] = useState("https://github.com/example/repo.git");
  const [sessionWorkflow, setSessionWorkflow] = useState<"chat" | "loop_n" | "loop_until_sentinel">("chat");
  const [sessionPrompt, setSessionPrompt] = useState("Hello");
  const [sessionAgentCli, setSessionAgentCli] = useState("cursor");
  const [sessionLoopN, setSessionLoopN] = useState("3");
  const [sessionSentinel, setSessionSentinel] = useState("DONE");
  const [sessionOut, setSessionOut] = useState("");
  const [sessionIdField, setSessionIdField] = useState("");
  const [sessionDetailOut, setSessionDetailOut] = useState("");
  const [sessionInputMessage, setSessionInputMessage] = useState("");
  const [sessionInputOut, setSessionInputOut] = useState("");
  const [logsOut, setLogsOut] = useState("");
  const [logsJobIdFilter, setLogsJobIdFilter] = useState("");
  const [logsLastN, setLogsLastN] = useState("");
  const [workerLogsTaskId, setWorkerLogsTaskId] = useState("");
  const [workerLogsJson, setWorkerLogsJson] = useState(
    '[{"timestamp":"2025-01-01T12:00:00.000Z","level":"info","message":"hello from UI","source":"worker"}]',
  );

  useEffect(() => {
    if (!base) {
      return;
    }
    let cancelled = false;

    async function run() {
      const next: ProbeRow[] = [];
      for (const row of [
        { label: "GET /health", path: "/health" },
        { label: "GET /ready", path: "/ready" },
        { label: "GET /health/idle", path: "/health/idle" },
      ]) {
        const url = `${base}${row.path}`;
        try {
          const res = await fetch(url, { method: "GET" });
          const text = await res.text();
          let detail = `${res.status}`;
          try {
            const j = JSON.parse(text) as Record<string, unknown>;
            detail = `${res.status} ${JSON.stringify(j)}`;
          } catch {
            if (text.length > 0 && text.length < 200) {
              detail = `${res.status} ${text}`;
            }
          }
          if (cancelled) {
            return;
          }
          next.push({
            label: row.label,
            path: row.path,
            state: res.ok ? "ok" : "error",
            detail,
          });
        } catch (e) {
          if (cancelled) {
            return;
          }
          const msg = e instanceof Error ? e.message : String(e);
          next.push({
            label: row.label,
            path: row.path,
            state: "error",
            detail: msg,
          });
        }
      }
      if (!cancelled) {
        setRows(next);
      }
    }

    void run();
    return () => {
      cancelled = true;
    };
  }, [base]);

  async function fetchIdentityJson(path: string): Promise<string> {
    if (!apiKey.trim()) {
      return "Set an API key to call authenticated identity endpoints.";
    }
    const url = `${base}${path}`;
    const res = await fetch(url, {
      headers: { Authorization: `Bearer ${apiKey.trim()}` },
    });
    const text = await res.text();
    try {
      const j = JSON.parse(text) as unknown;
      return `${res.status} ${JSON.stringify(j, null, 2)}`;
    } catch {
      return `${res.status} ${text}`;
    }
  }

  return (
    <main style={{ fontFamily: "system-ui, sans-serif", maxWidth: 720, margin: "0 auto", padding: "0 1rem" }}>
      <h1 style={{ fontWeight: 600 }}>API playground</h1>
      <p style={{ color: "#444", lineHeight: 1.5 }}>
        Control plane: <code>{base}</code> (set in <Link to="/settings">Settings</Link>).
      </p>
      <p style={{ color: "#444" }}>No-auth probes:</p>
      <ul style={{ listStyle: "none", padding: 0 }}>
        {rows.map((r) => (
          <li
            key={r.path}
            style={{
              border: "1px solid #ddd",
              borderRadius: 8,
              padding: "0.75rem 1rem",
              marginBottom: "0.5rem",
              background: r.state === "error" ? "#fff5f5" : r.state === "ok" ? "#f6fff8" : "#fafafa",
            }}
          >
            <div style={{ fontWeight: 500 }}>{r.label}</div>
            <div style={{ fontSize: "0.875rem", color: "#333", marginTop: 4 }}>{r.detail}</div>
          </li>
        ))}
      </ul>

      <section style={{ marginTop: "2rem" }}>
        <h2 style={{ fontWeight: 600, fontSize: "1.1rem" }}>Workers</h2>
        <p style={{ color: "#444", fontSize: "0.9rem", lineHeight: 1.5 }}>
          Same API key as identities. Register sends <code>client_version</code> semver; must match control plane{" "}
          <strong>major.minor</strong> (use <code>{CONTROL_PLANE_RELEASE}</code> for this repo release).
        </p>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", marginBottom: "0.75rem" }}>
        <button
          type="button"
          style={btn}
          onClick={async () => {
            if (!apiKey.trim()) {
              setWorkersOut("Set an API key first.");
              return;
            }
            const url = `${base}/workers`;
            const res = await fetch(url, { headers: { Authorization: `Bearer ${apiKey.trim()}` } });
            const text = await res.text();
            try {
              const j = JSON.parse(text) as unknown;
              setWorkersOut(`${res.status} ${JSON.stringify(j, null, 2)}`);
            } catch {
              setWorkersOut(`${res.status} ${text}`);
            }
          }}
        >
          GET /workers
        </button>
        </div>
        <p style={{ color: "#444", fontSize: "0.9rem", marginTop: "1rem", lineHeight: 1.5 }}>
          Pull / complete need a registered worker. Create a session below (identity must have agent + git tokens), then pull to claim the first job. For <code>loop_until_sentinel</code>, check “sentinel reached” on complete when the agent output contained the literal substring from session params.
        </p>
        <label style={{ display: "block", marginBottom: "0.35rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>Pull — worker id</span>
          <input
            value={workerPullId}
            onChange={(e) => setWorkerPullId(e.target.value)}
            placeholder="same id as register"
            style={{ width: "100%", maxWidth: 360, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <button
          type="button"
          style={{ ...btn, marginBottom: "0.75rem" }}
          onClick={async () => {
            if (!apiKey.trim()) {
              setWorkerPullOut("Set an API key first.");
              return;
            }
            const wid = workerPullId.trim();
            if (!wid) {
              setWorkerPullOut("Enter worker id.");
              return;
            }
            const url = `${base}/workers/tasks/pull`;
            const res = await fetch(url, {
              method: "POST",
              headers: {
                Authorization: `Bearer ${apiKey.trim()}`,
                "Content-Type": "application/json",
              },
              body: JSON.stringify({ worker_id: wid }),
            });
            const text = await res.text();
            if (res.status === 204) {
              setWorkerPullOut("204 No Content (no work)");
              return;
            }
            try {
              const j = JSON.parse(text) as unknown;
              setWorkerPullOut(`${res.status} ${JSON.stringify(j, null, 2)}`);
            } catch {
              setWorkerPullOut(`${res.status} ${text}`);
            }
          }}
        >
          POST /workers/tasks/pull
        </button>
        <label style={{ display: "block", marginBottom: "0.35rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>Complete — task id (job UUID)</span>
          <input
            value={workerCompleteTaskId}
            onChange={(e) => setWorkerCompleteTaskId(e.target.value)}
            placeholder="uuid from pull response"
            style={{ width: "100%", maxWidth: 420, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <label style={{ display: "block", marginBottom: "0.5rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>Complete — worker id (optional, recommended)</span>
          <input
            value={workerCompleteWorkerId}
            onChange={(e) => setWorkerCompleteWorkerId(e.target.value)}
            style={{ width: "100%", maxWidth: 360, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <label style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: "0.75rem", fontSize: "0.9rem", color: "#444" }}>
          <input
            type="checkbox"
            checked={workerCompleteSentinelReached}
            onChange={(e) => setWorkerCompleteSentinelReached(e.target.checked)}
          />
          sentinel_reached (loop_until_sentinel)
        </label>
        <button
          type="button"
          style={{ ...btn, marginBottom: "0.75rem" }}
          onClick={async () => {
            if (!apiKey.trim()) {
              setWorkerCompleteOut("Set an API key first.");
              return;
            }
            const tid = workerCompleteTaskId.trim();
            if (!tid) {
              setWorkerCompleteOut("Enter task id.");
              return;
            }
            const url = `${base}/workers/tasks/${encodeURIComponent(tid)}/complete`;
            const body: Record<string, unknown> = { status: "success" };
            const w = workerCompleteWorkerId.trim();
            if (w) {
              body.worker_id = w;
            }
            if (workerCompleteSentinelReached) {
              body.sentinel_reached = true;
            }
            const res = await fetch(url, {
              method: "POST",
              headers: {
                Authorization: `Bearer ${apiKey.trim()}`,
                "Content-Type": "application/json",
              },
              body: JSON.stringify(body),
            });
            const text = await res.text();
            try {
              const j = JSON.parse(text) as unknown;
              setWorkerCompleteOut(`${res.status} ${JSON.stringify(j, null, 2)}`);
            } catch {
              setWorkerCompleteOut(`${res.status} ${text}`);
            }
          }}
        >
          POST /workers/tasks/:id/complete (success)
        </button>
        {workerPullOut ? <pre style={{ ...preBox, marginTop: 8 }}>{workerPullOut}</pre> : null}
        {workerCompleteOut ? <pre style={{ ...preBox, marginTop: 8 }}>{workerCompleteOut}</pre> : null}
        <label style={{ display: "block", marginBottom: "0.35rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>Register worker id</span>
          <input
            value={workerRegId}
            onChange={(e) => setWorkerRegId(e.target.value)}
            placeholder="hostname-1"
            style={{ width: "100%", maxWidth: 360, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <label style={{ display: "block", marginBottom: "0.75rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>client_version (semver)</span>
          <input
            value={workerRegVersion}
            onChange={(e) => setWorkerRegVersion(e.target.value)}
            style={{ width: "100%", maxWidth: 200, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <button
          type="button"
          style={btn}
          onClick={async () => {
            if (!apiKey.trim()) {
              setWorkersOut("Set an API key first.");
              return;
            }
            const id = workerRegId.trim();
            if (!id) {
              setWorkersOut("Enter a worker id.");
              return;
            }
            const url = `${base}/workers/register`;
            const body = {
              id,
              host: typeof window !== "undefined" ? window.location.hostname : "web-ui",
              labels: { platform: "browser" },
              capabilities: [],
              client_version: workerRegVersion.trim() || CONTROL_PLANE_RELEASE,
            };
            const res = await fetch(url, {
              method: "POST",
              headers: {
                Authorization: `Bearer ${apiKey.trim()}`,
                "Content-Type": "application/json",
              },
              body: JSON.stringify(body),
            });
            const text = await res.text();
            try {
              const j = JSON.parse(text) as unknown;
              setWorkersOut(`${res.status} ${JSON.stringify(j, null, 2)}`);
            } catch {
              setWorkersOut(`${res.status} ${text}`);
            }
          }}
        >
          POST /workers/register
        </button>
        {workersOut ? <pre style={{ ...preBox, marginTop: 12 }}>{workersOut}</pre> : null}
      </section>

      <section style={{ marginTop: "2rem" }}>
        <h2 style={{ fontWeight: 600, fontSize: "1.1rem" }}>Sessions</h2>
        <p style={{ color: "#444", fontSize: "0.9rem", lineHeight: 1.5 }}>
          Uses the same API key as below (Identities section). Workflows: <code>chat</code> and <code>inbox</code> (follow-up via POST /sessions/:id/input when <code>running</code> and no active job),{" "}
          <code>loop_n</code>, <code>loop_until_sentinel</code> (literal substring; server caps iterations via <code>LOOP_UNTIL_SENTINEL_MAX_ITERATIONS</code>).
        </p>
        <label style={{ display: "block", marginBottom: "0.35rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>workflow</span>
          <select
            value={sessionWorkflow}
            onChange={(e) => setSessionWorkflow(e.target.value as typeof sessionWorkflow)}
            style={{ width: "100%", maxWidth: 320, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          >
            <option value="chat">chat</option>
            <option value="loop_n">loop_n</option>
            <option value="loop_until_sentinel">loop_until_sentinel</option>
          </select>
        </label>
        <label style={{ display: "block", marginBottom: "0.35rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>repo_url</span>
          <input
            value={sessionRepoUrl}
            onChange={(e) => setSessionRepoUrl(e.target.value)}
            style={{ width: "100%", maxWidth: 480, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <label style={{ display: "block", marginBottom: "0.35rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>prompt</span>
          <input
            value={sessionPrompt}
            onChange={(e) => setSessionPrompt(e.target.value)}
            style={{ width: "100%", maxWidth: 480, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <label style={{ display: "block", marginBottom: "0.75rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>agent_cli</span>
          <input
            value={sessionAgentCli}
            onChange={(e) => setSessionAgentCli(e.target.value)}
            placeholder="cursor or claude_code"
            style={{ width: "100%", maxWidth: 360, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        {sessionWorkflow === "loop_n" ? (
          <label style={{ display: "block", marginBottom: "0.75rem" }}>
            <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>n (iterations)</span>
            <input
              value={sessionLoopN}
              onChange={(e) => setSessionLoopN(e.target.value)}
              inputMode="numeric"
              style={{ width: "100%", maxWidth: 200, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
            />
          </label>
        ) : null}
        {sessionWorkflow === "loop_until_sentinel" ? (
          <label style={{ display: "block", marginBottom: "0.75rem" }}>
            <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>sentinel (literal substring)</span>
            <input
              value={sessionSentinel}
              onChange={(e) => setSessionSentinel(e.target.value)}
              style={{ width: "100%", maxWidth: 480, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
            />
          </label>
        ) : null}
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", marginBottom: "0.75rem" }}>
          <button
            type="button"
            style={btn}
            onClick={async () => {
              if (!apiKey.trim()) {
                setSessionOut("Set an API key first.");
                return;
              }
              const url = `${base}/sessions`;
              const nParsed = Number.parseInt(sessionLoopN.trim(), 10);
              const params: Record<string, string | number> = {
                prompt: sessionPrompt.trim(),
                agent_cli: sessionAgentCli.trim(),
              };
              if (sessionWorkflow === "loop_n") {
                if (!Number.isFinite(nParsed) || nParsed < 1) {
                  setSessionOut("loop_n requires a positive integer n.");
                  return;
                }
                params.n = nParsed;
              }
              if (sessionWorkflow === "loop_until_sentinel") {
                const s = sessionSentinel.trim();
                if (!s) {
                  setSessionOut("loop_until_sentinel requires a non-empty sentinel.");
                  return;
                }
                params.sentinel = s;
              }
              const res = await fetch(url, {
                method: "POST",
                headers: {
                  Authorization: `Bearer ${apiKey.trim()}`,
                  "Content-Type": "application/json",
                },
                body: JSON.stringify({
                  repo_url: sessionRepoUrl.trim(),
                  workflow: sessionWorkflow,
                  params,
                }),
              });
              const text = await res.text();
              try {
                const j = JSON.parse(text) as { session_id?: string };
                if (j.session_id) {
                  setSessionIdField(j.session_id);
                }
                setSessionOut(`${res.status} ${JSON.stringify(j, null, 2)}`);
              } catch {
                setSessionOut(`${res.status} ${text}`);
              }
            }}
          >
            POST /sessions
          </button>
          <button
            type="button"
            style={btn}
            onClick={async () => {
              if (!apiKey.trim()) {
                setSessionDetailOut("Set an API key first.");
                return;
              }
              const id = sessionIdField.trim();
              if (!id) {
                setSessionDetailOut("Enter session id (or create a session first).");
                return;
              }
              const url = `${base}/sessions/${encodeURIComponent(id)}`;
              const res = await fetch(url, { headers: { Authorization: `Bearer ${apiKey.trim()}` } });
              const text = await res.text();
              try {
                const j = JSON.parse(text) as unknown;
                setSessionDetailOut(`${res.status} ${JSON.stringify(j, null, 2)}`);
              } catch {
                setSessionDetailOut(`${res.status} ${text}`);
              }
            }}
          >
            GET /sessions/:id
          </button>
          <button
            type="button"
            style={btn}
            onClick={async () => {
              if (!apiKey.trim()) {
                setSessionDetailOut("Set an API key first.");
                return;
              }
              const id = sessionIdField.trim();
              if (!id) {
                setSessionDetailOut("Enter session id.");
                return;
              }
              if (!window.confirm(`Delete session ${id} from the control plane? This removes jobs and central logs for that session.`)) {
                return;
              }
              const url = `${base}/sessions/${encodeURIComponent(id)}`;
              const res = await fetch(url, {
                method: "DELETE",
                headers: { Authorization: `Bearer ${apiKey.trim()}` },
              });
              const text = await res.text();
              setSessionDetailOut(
                res.status === 204 ? `${res.status} (session deleted)` : `${res.status} ${text}`,
              );
            }}
          >
            DELETE /sessions/:id
          </button>
          <button
            type="button"
            style={btn}
            onClick={async () => {
              if (!apiKey.trim()) {
                setSessionDetailOut("Set an API key first.");
                return;
              }
              const url = `${base}/sessions`;
              const res = await fetch(url, { headers: { Authorization: `Bearer ${apiKey.trim()}` } });
              const text = await res.text();
              try {
                const j = JSON.parse(text) as unknown;
                setSessionDetailOut(`${res.status} ${JSON.stringify(j, null, 2)}`);
              } catch {
                setSessionDetailOut(`${res.status} ${text}`);
              }
            }}
          >
            GET /sessions
          </button>
        </div>
        <label style={{ display: "block", marginBottom: "0.35rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>Session id</span>
          <input
            value={sessionIdField}
            onChange={(e) => setSessionIdField(e.target.value)}
            placeholder="from create response"
            style={{ width: "100%", maxWidth: 480, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <label style={{ display: "block", marginBottom: "0.35rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>Follow-up message</span>
          <input
            value={sessionInputMessage}
            onChange={(e) => setSessionInputMessage(e.target.value)}
            style={{ width: "100%", maxWidth: 480, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <button
          type="button"
          style={{ ...btn, marginBottom: "0.75rem" }}
          onClick={async () => {
            if (!apiKey.trim()) {
              setSessionInputOut("Set an API key first.");
              return;
            }
            const id = sessionIdField.trim();
            if (!id) {
              setSessionInputOut("Enter session id.");
              return;
            }
            const url = `${base}/sessions/${encodeURIComponent(id)}/input`;
            const res = await fetch(url, {
              method: "POST",
              headers: {
                Authorization: `Bearer ${apiKey.trim()}`,
                "Content-Type": "application/json",
              },
              body: JSON.stringify({ message: sessionInputMessage.trim() }),
            });
            const text = await res.text();
            try {
              const j = JSON.parse(text) as unknown;
              setSessionInputOut(`${res.status} ${JSON.stringify(j, null, 2)}`);
            } catch {
              setSessionInputOut(`${res.status} ${text}`);
            }
          }}
        >
          POST /sessions/:id/input
        </button>
        {sessionOut ? <pre style={preBox}>{sessionOut}</pre> : null}
        {sessionDetailOut ? <pre style={preBox}>{sessionDetailOut}</pre> : null}
        {sessionInputOut ? <pre style={preBox}>{sessionInputOut}</pre> : null}
      </section>

      <section style={{ marginTop: "2rem" }}>
        <h2 style={{ fontWeight: 600, fontSize: "1.1rem" }}>Logs</h2>
        <p style={{ color: "#444", fontSize: "0.9rem", lineHeight: 1.5 }}>
          Same session id as above. <code>GET /sessions/:id/logs</code> and <code>DELETE /sessions/:id/logs</code>; workers post batches to{" "}
          <code>POST /workers/tasks/:id/logs</code> (task id = job UUID from pull). Timestamps must be RFC 3339.
        </p>
        <label style={{ display: "block", marginBottom: "0.35rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>Optional job_id filter (list / delete)</span>
          <input
            value={logsJobIdFilter}
            onChange={(e) => setLogsJobIdFilter(e.target.value)}
            placeholder="uuid — omit for all session logs"
            style={{ width: "100%", maxWidth: 480, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <label style={{ display: "block", marginBottom: "0.75rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>last (newest N lines, chronological)</span>
          <input
            value={logsLastN}
            onChange={(e) => setLogsLastN(e.target.value)}
            placeholder="e.g. 50 — leave empty for cursor pagination"
            style={{ width: "100%", maxWidth: 200, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", marginBottom: "0.75rem" }}>
          <button
            type="button"
            style={btn}
            onClick={async () => {
              if (!apiKey.trim()) {
                setLogsOut("Set an API key first.");
                return;
              }
              const id = sessionIdField.trim();
              if (!id) {
                setLogsOut("Enter session id.");
                return;
              }
              const u = new URL(`${base}/sessions/${encodeURIComponent(id)}/logs`);
              const j = logsJobIdFilter.trim();
              if (j) {
                u.searchParams.set("job_id", j);
              }
              const last = logsLastN.trim();
              if (last) {
                u.searchParams.set("last", last);
              } else {
                u.searchParams.set("limit", "50");
              }
              const res = await fetch(u.toString(), { headers: { Authorization: `Bearer ${apiKey.trim()}` } });
              const text = await res.text();
              try {
                const parsed = JSON.parse(text) as unknown;
                setLogsOut(`${res.status} ${JSON.stringify(parsed, null, 2)}`);
              } catch {
                setLogsOut(`${res.status} ${text}`);
              }
            }}
          >
            GET /sessions/:id/logs
          </button>
          <button
            type="button"
            style={btn}
            onClick={async () => {
              if (!apiKey.trim()) {
                setLogsOut("Set an API key first.");
                return;
              }
              const id = sessionIdField.trim();
              if (!id) {
                setLogsOut("Enter session id.");
                return;
              }
              const u = new URL(`${base}/sessions/${encodeURIComponent(id)}/logs`);
              const j = logsJobIdFilter.trim();
              if (j) {
                u.searchParams.set("job_id", j);
              }
              const res = await fetch(u.toString(), {
                method: "DELETE",
                headers: { Authorization: `Bearer ${apiKey.trim()}` },
              });
              const text = await res.text();
              setLogsOut(text.length > 0 ? `${res.status} ${text}` : `${res.status}`);
            }}
          >
            DELETE /sessions/:id/logs
          </button>
        </div>
        <label style={{ display: "block", marginBottom: "0.35rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>Worker POST — task id (job UUID)</span>
          <input
            value={workerLogsTaskId}
            onChange={(e) => setWorkerLogsTaskId(e.target.value)}
            placeholder="from pull response"
            style={{ width: "100%", maxWidth: 480, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <label style={{ display: "block", marginBottom: "0.5rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>JSON array body</span>
          <textarea
            value={workerLogsJson}
            onChange={(e) => setWorkerLogsJson(e.target.value)}
            rows={3}
            style={{ width: "100%", maxWidth: 560, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc", fontFamily: "monospace", fontSize: "0.8rem" }}
          />
        </label>
        <button
          type="button"
          style={{ ...btn, marginBottom: "0.75rem" }}
          onClick={async () => {
            if (!apiKey.trim()) {
              setLogsOut("Set an API key first.");
              return;
            }
            const tid = workerLogsTaskId.trim();
            if (!tid) {
              setLogsOut("Enter task id.");
              return;
            }
            let body: unknown;
            try {
              body = JSON.parse(workerLogsJson) as unknown;
            } catch (e) {
              setLogsOut(e instanceof Error ? e.message : "Invalid JSON");
              return;
            }
            const url = `${base}/workers/tasks/${encodeURIComponent(tid)}/logs`;
            const res = await fetch(url, {
              method: "POST",
              headers: {
                Authorization: `Bearer ${apiKey.trim()}`,
                "Content-Type": "application/json",
              },
              body: JSON.stringify(body),
            });
            const text = await res.text();
            try {
              const parsed = JSON.parse(text) as unknown;
              setLogsOut(`${res.status} ${JSON.stringify(parsed, null, 2)}`);
            } catch {
              setLogsOut(`${res.status} ${text}`);
            }
          }}
        >
          POST /workers/tasks/:id/logs
        </button>
        {logsOut ? <pre style={preBox}>{logsOut}</pre> : null}
      </section>

      <section style={{ marginTop: "2rem" }}>
        <h2 style={{ fontWeight: 600, fontSize: "1.1rem" }}>Identities (BYOL)</h2>
        <p style={{ color: "#444", fontSize: "0.9rem", lineHeight: 1.5 }}>
          Uses the API key from <Link to="/settings">Settings</Link>. Token values are never returned from the server; patch fields stay in the browser until you submit.
        </p>
        {!apiKey.trim() ? (
          <p style={{ color: "#a33", fontSize: "0.9rem", marginBottom: "0.75rem" }}>
            No API key configured — open Settings to add one.
          </p>
        ) : null}
        <label style={{ display: "block", marginBottom: "0.75rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555", marginBottom: 4 }}>Identity id</span>
          <input
            value={identityId}
            onChange={(e) => setIdentityId(e.target.value)}
            onBlur={() => writeStoredGitIdentityId(identityId)}
            style={{ width: "100%", maxWidth: 240, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>

        <p style={{ color: "#444", fontSize: "0.9rem", lineHeight: 1.5, marginBottom: "0.75rem" }}>
          Git via OAuth (no API key for the redirect): prefer <Link to="/settings">Settings → Git sign-in</Link> for the same
          flow. Here you can exercise the links and identity APIs. Configure provider env vars and{" "}
          <code>REDIRECT_AFTER_AUTH</code> on the server (see repo README). After success you land on your redirect URL with{" "}
          <code>oauth_success</code> in the query string.
        </p>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", marginBottom: "1rem" }}>
          <a
            href={`${base}/auth/github?identity_id=${encodeURIComponent(identityId.trim() || "default")}`}
            style={{ ...btn, textDecoration: "none", display: "inline-block", color: "#111" }}
            onClick={() => writeStoredGitIdentityId(identityId)}
          >
            Sign in with GitHub
          </a>
          <a
            href={`${base}/auth/gitlab?identity_id=${encodeURIComponent(identityId.trim() || "default")}`}
            style={{ ...btn, textDecoration: "none", display: "inline-block", color: "#111" }}
            onClick={() => writeStoredGitIdentityId(identityId)}
          >
            Sign in with GitLab
          </a>
        </div>

        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", marginBottom: "0.75rem" }}>
          <button
            type="button"
            style={btn}
            onClick={async () => setIdentityGetOut(await fetchIdentityJson(`/identities/${encodeURIComponent(identityId.trim() || "default")}`))}
          >
            GET credentials
          </button>
          <button
            type="button"
            style={btn}
            onClick={async () =>
              setIdentityAuthOut(await fetchIdentityJson(`/identities/${encodeURIComponent(identityId.trim() || "default")}/auth-status`))
            }
          >
            GET auth status
          </button>
        </div>
        {(identityGetOut || identityAuthOut) && (
          <div style={{ fontSize: "0.85rem", marginBottom: "1rem" }}>
            {identityGetOut ? (
              <pre style={preBox}>{identityGetOut}</pre>
            ) : null}
            {identityAuthOut ? (
              <pre style={preBox}>{identityAuthOut}</pre>
            ) : null}
          </div>
        )}

        <div style={{ marginBottom: "0.5rem" }}>
          <span style={{ fontSize: "0.85rem", color: "#555", marginRight: 8 }}>Repo list provider</span>
          <select value={repoProvider} onChange={(e) => setRepoProvider(e.target.value)} style={{ padding: "0.35rem 0.5rem", borderRadius: 6 }}>
            <option value="github">github</option>
            <option value="gitlab">gitlab</option>
          </select>
          <button
            type="button"
            style={{ ...btn, marginLeft: 8 }}
            onClick={async () => {
              const id = encodeURIComponent(identityId.trim() || "default");
              const q = `?provider=${encodeURIComponent(repoProvider)}`;
              setIdentityReposOut(await fetchIdentityJson(`/identities/${id}/repositories${q}`));
            }}
          >
            GET repositories
          </button>
        </div>
        {identityReposOut ? <pre style={preBox}>{identityReposOut}</pre> : null}

        <h3 style={{ fontWeight: 600, fontSize: "1rem", marginTop: "1.25rem" }}>PATCH tokens</h3>
        <label style={{ display: "block", marginBottom: "0.5rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>agent_token</span>
          <input
            type="password"
            autoComplete="off"
            value={identityPatchAgent}
            onChange={(e) => setIdentityPatchAgent(e.target.value)}
            style={{ width: "100%", maxWidth: 480, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <label style={{ display: "block", marginBottom: "0.75rem" }}>
          <span style={{ display: "block", fontSize: "0.85rem", color: "#555" }}>git_token</span>
          <input
            type="password"
            autoComplete="off"
            value={identityPatchGit}
            onChange={(e) => setIdentityPatchGit(e.target.value)}
            style={{ width: "100%", maxWidth: 480, padding: "0.5rem 0.6rem", borderRadius: 6, border: "1px solid #ccc" }}
          />
        </label>
        <button
          type="button"
          style={btn}
          onClick={async () => {
            if (!apiKey.trim()) {
              setIdentityPatchOut("Set an API key first.");
              return;
            }
            const body: Record<string, string> = {};
            if (identityPatchAgent.trim()) {
              body.agent_token = identityPatchAgent.trim();
            }
            if (identityPatchGit.trim()) {
              body.git_token = identityPatchGit.trim();
            }
            if (Object.keys(body).length === 0) {
              setIdentityPatchOut("Enter at least one token to PATCH.");
              return;
            }
            const id = encodeURIComponent(identityId.trim() || "default");
            const url = `${base}/identities/${id}`;
            const res = await fetch(url, {
              method: "PATCH",
              headers: {
                Authorization: `Bearer ${apiKey.trim()}`,
                "Content-Type": "application/json",
              },
              body: JSON.stringify(body),
            });
            const text = await res.text();
            setIdentityPatchOut(text.length > 0 ? `${res.status} ${text}` : `${res.status}`);
          }}
        >
          PATCH identity
        </button>
        {identityPatchOut ? <pre style={{ ...preBox, marginTop: 8 }}>{identityPatchOut}</pre> : null}
      </section>
    </main>
  );
}

const btn: CSSProperties = {
  padding: "0.45rem 0.85rem",
  borderRadius: 6,
  border: "1px solid #bbb",
  background: "#f8f8f8",
  cursor: "pointer",
  fontSize: "0.875rem",
};

const preBox: CSSProperties = {
  background: "#f4f4f4",
  padding: "0.75rem",
  borderRadius: 6,
  overflow: "auto",
  fontSize: "0.8rem",
  marginTop: 8,
};
