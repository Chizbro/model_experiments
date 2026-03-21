import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { Link, useParams } from "react-router-dom";
import { ControlPlaneHttpError } from "../api/client";
import { postAgentInbox } from "../api/agents";
import {
  deleteSessionLogs,
  getSession,
  patchJobRetain,
  patchSessionRetain,
  sendSessionInput,
} from "../api/sessions";
import { useSessionEventsStream } from "../hooks/useSessionEventsStream";
import { useSessionLogsStream } from "../hooks/useSessionLogsStream";
import { useSettings } from "../hooks/useSettings";
import { sessionJobOutcomeNotes } from "../lib/sessionJobHints";

type LogLevelFilter = "" | "debug" | "info" | "warn" | "error";

function inboxAgentIdFromParams(params: Record<string, unknown>): string {
  const v = params.agent_id;
  return typeof v === "string" ? v.trim() : "";
}

export function SessionDetailPage() {
  const { sessionId } = useParams<{ sessionId: string }>();
  const { controlPlaneUrl, apiKey } = useSettings();
  const qc = useQueryClient();
  const base = controlPlaneUrl ?? "";
  const key = apiKey.trim();
  const id = sessionId?.trim() ?? "";

  const [jobLogFilter, setJobLogFilter] = useState("");
  const [logLevel, setLogLevel] = useState<LogLevelFilter>("");
  const [logRefresh, setLogRefresh] = useState(0);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [chatMessage, setChatMessage] = useState("");
  const [chatBusy, setChatBusy] = useState(false);
  const [retainBusy, setRetainBusy] = useState<string | null>(null);

  const q = useQuery({
    queryKey: ["session", base, key, id],
    queryFn: () => getSession(base, key, id),
    enabled: Boolean(base && key && id),
  });

  const s = q.data;
  const terminal = s ? s.status === "completed" || s.status === "failed" : false;
  const streamEnabled = Boolean(base && key && id && s && !terminal);

  const logsStream = useSessionLogsStream({
    baseUrl: base,
    apiKey: key,
    sessionId: id,
    jobId: jobLogFilter.trim() || null,
    level: logLevel.trim() || null,
    enabled: streamEnabled,
    refreshKey: logRefresh,
  });

  const eventsStream = useSessionEventsStream({
    baseUrl: base,
    apiKey: key,
    sessionId: id,
    enabled: streamEnabled,
  });

  if (!id) {
    return <p className="text-sm text-destructive">Missing session id.</p>;
  }

  if (q.isPending) {
    return <p className="text-sm text-muted">Loading session…</p>;
  }

  if (q.isError) {
    const msg =
      q.error instanceof ControlPlaneHttpError
        ? `${q.error.mapped.title}: ${q.error.mapped.detail}`
        : (q.error as Error).message;
    return (
      <div className="space-y-3">
        <p className="text-sm text-destructive">{msg}</p>
        <Link className="text-sm text-primary underline underline-offset-2" to="/sessions">
          Back to sessions
        </Link>
      </div>
    );
  }

  const sessionRetain = s!.retain_forever ?? false;
  const jobsWithErrors = s!.jobs.filter((j) => (j.error_message ?? "").trim().length > 0);

  async function onToggleSessionRetain(next: boolean) {
    setActionError(null);
    setRetainBusy("session");
    try {
      await patchSessionRetain(base, key, id, next);
      await qc.invalidateQueries({ queryKey: ["session", base, key, id] });
    } catch (e) {
      const msg =
        e instanceof ControlPlaneHttpError
          ? `${e.mapped.title}: ${e.mapped.detail}`
          : e instanceof Error
            ? e.message
            : String(e);
      setActionError(msg);
    } finally {
      setRetainBusy(null);
    }
  }

  async function onToggleJobRetain(jobId: string, next: boolean) {
    setActionError(null);
    setRetainBusy(jobId);
    try {
      await patchJobRetain(base, key, id, jobId, next);
      await qc.invalidateQueries({ queryKey: ["session", base, key, id] });
    } catch (e) {
      const msg =
        e instanceof ControlPlaneHttpError
          ? `${e.mapped.title}: ${e.mapped.detail}`
          : e instanceof Error
            ? e.message
            : String(e);
      setActionError(msg);
    } finally {
      setRetainBusy(null);
    }
  }

  async function onDeleteLogs() {
    const scope = jobLogFilter.trim()
      ? `logs for job ${jobLogFilter.trim().slice(0, 8)}…`
      : "all logs for this session";
    if (!window.confirm(`Delete ${scope}? This cannot be undone in the control plane.`)) {
      return;
    }
    setActionError(null);
    setDeleteBusy(true);
    try {
      await deleteSessionLogs(base, key, id, jobLogFilter.trim() || undefined);
      setLogRefresh((n) => n + 1);
      await qc.invalidateQueries({ queryKey: ["session", base, key, id] });
    } catch (e) {
      const msg =
        e instanceof ControlPlaneHttpError
          ? `${e.mapped.title}: ${e.mapped.detail}`
          : e instanceof Error
            ? e.message
            : String(e);
      setActionError(msg);
    } finally {
      setDeleteBusy(false);
    }
  }

  async function onSendChat(ev: FormEvent) {
    ev.preventDefault();
    const msg = chatMessage.trim();
    if (!msg) return;
    setActionError(null);
    setChatBusy(true);
    try {
      if (s!.workflow === "inbox") {
        const agentId = inboxAgentIdFromParams(s!.params);
        if (!agentId) {
          setActionError("Missing params.agent_id on this inbox session.");
          return;
        }
        await postAgentInbox(base, key, agentId, { payload: { message: msg } });
      } else {
        await sendSessionInput(base, key, id, msg);
      }
      setChatMessage("");
      await qc.invalidateQueries({ queryKey: ["session", base, key, id] });
    } catch (e) {
      const err =
        e instanceof ControlPlaneHttpError
          ? `${e.mapped.title}: ${e.mapped.detail}`
          : e instanceof Error
            ? e.message
            : String(e);
      setActionError(err);
    } finally {
      setChatBusy(false);
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Session</h1>
          <p className="mt-1 font-mono text-xs text-muted">{s!.session_id}</p>
        </div>
        <Link to="/sessions" className="text-sm text-primary underline underline-offset-2">
          All sessions
        </Link>
      </div>

      {actionError ? (
        <p className="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm text-destructive">
          {actionError}
        </p>
      ) : null}

      {jobsWithErrors.length > 0 ? (
        <div
          className="rounded-lg border border-destructive/45 bg-destructive/10 px-4 py-3 text-sm shadow-sm dark:bg-destructive/15"
          role="alert"
        >
          <p className="font-semibold text-destructive">Job errors</p>
          <p className="mt-1 text-xs text-muted">
            Always surface <code className="text-foreground">error_message</code> when set (CLIENT_EXPERIENCE §8). Check{" "}
            <span className="text-foreground">Settings → Credentials</span> if the text mentions Git or auth.
          </p>
          <ul className="mt-2 space-y-2">
            {jobsWithErrors.map((j) => (
              <li key={j.job_id} className="rounded-md border border-destructive/25 bg-background/80 px-3 py-2">
                <span className="font-mono text-xs text-muted">{j.job_id.slice(0, 8)}…</span>
                <p className="mt-1 font-medium text-destructive">{j.error_message}</p>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {s!.workflow === "chat" && s!.chat_history_truncated ? (
        <div className="rounded-lg border border-amber-200/90 bg-amber-50 px-4 py-3 text-sm shadow-sm dark:border-amber-900/50 dark:bg-amber-950/35">
          <p className="font-semibold text-amber-950 dark:text-amber-100">Long conversation</p>
          <p className="mt-1 text-amber-900/90 dark:text-amber-100/85">
            Only the most recent {s!.chat_history_max_turns ?? "N"} user and assistant turns (each side) are included in
            the next agent run; full history may still appear in logs (CLIENT_EXPERIENCE §12).
          </p>
        </div>
      ) : null}

      <dl className="grid gap-3 rounded-lg border border-border bg-card p-4 text-sm shadow-sm sm:grid-cols-2">
        <div>
          <dt className="text-muted">Status</dt>
          <dd className="font-medium">{s!.status}</dd>
        </div>
        <div>
          <dt className="text-muted">Workflow</dt>
          <dd>{s!.workflow}</dd>
        </div>
        <div className="sm:col-span-2">
          <dt className="text-muted">Repo</dt>
          <dd className="break-all font-mono text-xs">{s!.repo_url}</dd>
        </div>
        <div>
          <dt className="text-muted">Ref</dt>
          <dd className="font-mono text-xs">{s!.ref}</dd>
        </div>
        <div>
          <dt className="text-muted">Updated</dt>
          <dd className="text-muted">{s!.updated_at}</dd>
        </div>
        <div className="flex items-center gap-2 sm:col-span-2">
          <input
            id="retain-session"
            type="checkbox"
            className="h-4 w-4 rounded border-border"
            checked={sessionRetain}
            disabled={retainBusy === "session"}
            onChange={(e) => void onToggleSessionRetain(e.target.checked)}
          />
          <label htmlFor="retain-session" className="text-sm">
            Retain session logs forever (exempt from retention purge)
          </label>
        </div>
      </dl>

      {(s!.workflow === "chat" || s!.workflow === "inbox") && s!.status === "running" ? (
        <section className="space-y-2 rounded-lg border border-border bg-card p-4 shadow-sm">
          <h2 className="text-lg font-semibold">{s!.workflow === "inbox" ? "Inbox message" : "Chat input"}</h2>
          <p className="text-xs text-muted">
            {s!.workflow === "inbox" ? (
              <>
                Sends <code className="text-xs">POST /agents/{inboxAgentIdFromParams(s!.params) || "…"}/inbox</code> with{" "}
                <code className="text-xs">payload.message</code>. The worker must call{" "}
                <code className="text-xs">POST /workers/:id/inbox-listener</code> for this <code className="text-xs">agent_id</code>
                ; <code className="text-xs">POST /workers/tasks/pull</code> then promotes the queue row into a job (see{" "}
                <code className="text-xs">docs/API_OVERVIEW.md</code> §8).
              </>
            ) : (
              <>
                Sends <code className="text-xs">POST /sessions/:id/input</code> — a job is enqueued when no job is pending or
                assigned.
              </>
            )}
          </p>
          <form className="flex flex-col gap-2 sm:flex-row sm:items-end" onSubmit={(e) => void onSendChat(e)}>
            <textarea
              className="min-h-[80px] flex-1 rounded-md border border-border bg-background px-3 py-2 text-sm"
              placeholder={s!.workflow === "inbox" ? "Message to enqueue for this inbox…" : "Message for the agent…"}
              value={chatMessage}
              onChange={(e) => setChatMessage(e.target.value)}
              disabled={chatBusy}
            />
            <button
              type="submit"
              className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground disabled:opacity-50"
              disabled={chatBusy || !chatMessage.trim()}
            >
              {chatBusy ? "Sending…" : "Send"}
            </button>
          </form>
        </section>
      ) : null}

      <div>
        <h2 className="text-lg font-semibold">Jobs</h2>
        <p className="mt-1 text-sm text-muted">
          Git and PR/MR outcomes follow CLIENT_EXPERIENCE §8 — show <code className="text-xs">error_message</code>{" "}
          prominently when set; never stay silent when a PR was expected but <code className="text-xs">pull_request_url</code>{" "}
          is missing.
        </p>
        <div className="mt-3 overflow-x-auto rounded-lg border border-border bg-card shadow-sm">
          <table className="w-full min-w-[880px] border-collapse text-left text-sm">
            <thead className="border-b border-border bg-black/[0.02] text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-3 py-2 font-medium">Job</th>
                <th className="px-3 py-2 font-medium">Status</th>
                <th className="px-3 py-2 font-medium">Created</th>
                <th className="px-3 py-2 font-medium">Retain</th>
                <th className="px-3 py-2 font-medium">Error</th>
                <th className="px-3 py-2 font-medium">Commit</th>
                <th className="px-3 py-2 font-medium">PR / MR</th>
                <th className="px-3 py-2 font-medium">Note</th>
              </tr>
            </thead>
            <tbody>
              {s!.jobs.length === 0 ? (
                <tr>
                  <td colSpan={8} className="px-3 py-4 text-muted">
                    No jobs yet.
                  </td>
                </tr>
              ) : (
                s!.jobs.map((j) => {
                  const notes = sessionJobOutcomeNotes(j, s!.params);
                  const jr = j.retain_forever ?? false;
                  const err = (j.error_message ?? "").trim();
                  const rowTone = err ? "bg-destructive/[0.06] dark:bg-destructive/10" : "";
                  return (
                    <tr key={j.job_id} className={`border-b border-border last:border-0 ${rowTone}`}>
                      <td className="px-3 py-2 font-mono text-xs">{j.job_id.slice(0, 8)}…</td>
                      <td className="px-3 py-2">{j.status}</td>
                      <td className="whitespace-nowrap px-3 py-2 text-muted">{j.created_at}</td>
                      <td className="px-3 py-2">
                        <input
                          type="checkbox"
                          className="h-4 w-4 rounded border-border"
                          checked={jr}
                          disabled={retainBusy === j.job_id}
                          aria-label={`Retain logs for job ${j.job_id}`}
                          onChange={(e) => void onToggleJobRetain(j.job_id, e.target.checked)}
                        />
                      </td>
                      <td className="max-w-[240px] px-3 py-2 align-top">
                        {err ? (
                          <span className="font-medium text-destructive">{j.error_message}</span>
                        ) : (
                          <span className="text-muted">—</span>
                        )}
                      </td>
                      <td className="max-w-[120px] truncate px-3 py-2 font-mono text-xs text-muted">
                        {j.commit_ref ? `${j.commit_ref.slice(0, 10)}…` : "—"}
                      </td>
                      <td className="max-w-[200px] truncate px-3 py-2">
                        {j.pull_request_url ? (
                          <a className="text-primary underline underline-offset-2" href={j.pull_request_url}>
                            link
                          </a>
                        ) : (
                          "—"
                        )}
                      </td>
                      <td className="max-w-[300px] px-3 py-2 text-xs text-muted">{notes.trim() || "—"}</td>
                    </tr>
                  );
                })
              )}
            </tbody>
          </table>
        </div>
      </div>

      <section className="space-y-3 rounded-lg border border-border bg-card p-4 shadow-sm">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-lg font-semibold">Logs</h2>
          {logsStream.reconnecting ? (
            <span className="text-sm text-amber-700 dark:text-amber-400">Reconnecting…</span>
          ) : null}
        </div>
        <p className="text-xs text-muted">
          Loads full <code className="text-xs">GET /sessions/:id/logs</code> history (paginated), then{" "}
          <code className="text-xs">logs/stream</code>. On reconnect, history is merged by <code className="text-xs">id</code>{" "}
          (CLIENT_EXPERIENCE §4).
        </p>
        <div className="flex flex-wrap gap-3 text-sm">
          <label className="flex flex-col gap-1">
            <span className="text-xs text-muted">Job filter</span>
            <select
              className="rounded-md border border-border bg-background px-2 py-1 font-mono text-xs"
              value={jobLogFilter}
              onChange={(e) => setJobLogFilter(e.target.value)}
            >
              <option value="">All jobs</option>
              {s!.jobs.map((j) => (
                <option key={j.job_id} value={j.job_id}>
                  {j.job_id.slice(0, 8)}… ({j.status})
                </option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-xs text-muted">Level</span>
            <select
              className="rounded-md border border-border bg-background px-2 py-1 text-xs"
              value={logLevel}
              onChange={(e) => setLogLevel(e.target.value as LogLevelFilter)}
            >
              <option value="">All levels</option>
              <option value="debug">debug</option>
              <option value="info">info</option>
              <option value="warn">warn</option>
              <option value="error">error</option>
            </select>
          </label>
          <div className="flex items-end">
            <button
              type="button"
              className="rounded-md border border-border px-3 py-1 text-sm text-destructive hover:bg-destructive/5 disabled:opacity-50"
              disabled={deleteBusy}
              onClick={() => void onDeleteLogs()}
            >
              {deleteBusy ? "Deleting…" : "Delete logs…"}
            </button>
          </div>
        </div>
        {logsStream.loadingHistory ? (
          <p className="text-sm text-muted">Loading log history…</p>
        ) : null}
        {logsStream.historyError ? (
          <p className="text-sm text-destructive">{logsStream.historyError}</p>
        ) : null}
        {logsStream.streamError ? (
          <p className="text-sm text-destructive">Stream: {logsStream.streamError}</p>
        ) : null}
        {terminal ? (
          <p className="text-xs text-muted">Session finished — live streams stopped; history above reflects stored logs.</p>
        ) : null}
        <div className="max-h-[420px] overflow-auto rounded-md border border-border bg-black/[0.03] p-2 font-mono text-xs dark:bg-white/[0.04]">
          {logsStream.logs.length === 0 && !logsStream.loadingHistory ? (
            <p className="text-muted">No log lines yet.</p>
          ) : (
            <ul className="space-y-1">
              {logsStream.logs.map((line) => (
                <li key={line.id} className="whitespace-pre-wrap break-words">
                  <span className="text-muted">{line.timestamp}</span>{" "}
                  <span className="font-medium">{line.level}</span>{" "}
                  <span className="text-muted">{line.source}</span>{" "}
                  {line.job_id ? <span className="text-muted">job={line.job_id.slice(0, 8)}… </span> : null}
                  {line.message}
                </li>
              ))}
            </ul>
          )}
        </div>
      </section>

      <section className="space-y-3 rounded-lg border border-border bg-card p-4 shadow-sm">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-lg font-semibold">Session events (attach)</h2>
          {eventsStream.reconnecting ? (
            <span className="text-sm text-amber-700 dark:text-amber-400">Reconnecting…</span>
          ) : null}
        </div>
        <p className="text-xs text-muted">
          <code className="text-xs">GET /sessions/:id/events</code> — SSE <code className="text-xs">session_event</code>{" "}
          (see <code className="text-xs">docs/SSE_EVENTS.md</code>).
        </p>
        {eventsStream.error ? (
          <p className="text-sm text-destructive">{eventsStream.error}</p>
        ) : null}
        <ul className="max-h-[220px] space-y-1 overflow-auto rounded-md border border-border bg-black/[0.03] p-2 font-mono text-xs dark:bg-white/[0.04]">
          {eventsStream.events.length === 0 ? (
            <li className="text-muted">No lifecycle events yet.</li>
          ) : (
            eventsStream.events.map((ev, i) => (
              <li key={`${i}-${ev.event}-${ev.job_id ?? ""}`} className="break-words">
                <span className="font-medium">{ev.event}</span>
                {ev.job_id ? <span className="text-muted"> job={ev.job_id.slice(0, 8)}…</span> : null}{" "}
                <span className="text-muted">{JSON.stringify(ev.payload)}</span>
              </li>
            ))
          )}
        </ul>
      </section>
    </div>
  );
}
