import { useInfiniteQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { ControlPlaneHttpError } from "../api/client";
import { listSessionsPage } from "../api/sessions";
import type { SessionSummary } from "../api/types";
import { useSettings } from "../hooks/useSettings";
import { useMemo, useState } from "react";

export function SessionsListPage() {
  const { controlPlaneUrl, apiKey } = useSettings();
  const base = controlPlaneUrl!;
  const key = apiKey.trim();
  const [statusDraft, setStatusDraft] = useState("");
  const [statusApplied, setStatusApplied] = useState("");

  const q = useInfiniteQuery({
    queryKey: ["sessions", base, key, statusApplied],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) =>
      listSessionsPage(base, key, {
        limit: 20,
        cursor: pageParam,
        status: statusApplied || null,
      }),
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    enabled: Boolean(base && key),
  });

  const rows: SessionSummary[] = useMemo(() => q.data?.pages.flatMap((p) => p.items) ?? [], [q.data]);

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Sessions</h1>
          <p className="mt-1 max-w-2xl text-sm text-muted">Paginated from GET /sessions (cursor-based).</p>
        </div>
        <Link
          to="/sessions/new"
          className="inline-flex rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-fg shadow-sm hover:opacity-95"
        >
          New session
        </Link>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <label className="text-sm text-muted" htmlFor="sess-status">
          Status filter
        </label>
        <input
          id="sess-status"
          className="rounded-md border border-border bg-card px-2 py-1.5 text-sm"
          placeholder="e.g. running (optional)"
          value={statusDraft}
          onChange={(e) => setStatusDraft(e.target.value)}
        />
        <button
          type="button"
          className="rounded-md border border-border px-2 py-1.5 text-sm hover:bg-black/[0.03]"
          onClick={() => setStatusApplied(statusDraft.trim())}
        >
          Apply
        </button>
      </div>

      {q.isError ? (
        <p className="text-sm text-destructive">
          {q.error instanceof ControlPlaneHttpError
            ? `${q.error.mapped.title}: ${q.error.mapped.detail}`
            : (q.error as Error).message}
        </p>
      ) : null}

      <div className="overflow-x-auto rounded-lg border border-border bg-card shadow-sm">
        <table className="w-full min-w-[640px] border-collapse text-left text-sm">
          <thead className="border-b border-border bg-black/[0.02] text-xs uppercase tracking-wide text-muted">
            <tr>
              <th className="px-3 py-2 font-medium">Session</th>
              <th className="px-3 py-2 font-medium">Repo</th>
              <th className="px-3 py-2 font-medium">Ref</th>
              <th className="px-3 py-2 font-medium">Workflow</th>
              <th className="px-3 py-2 font-medium">Status</th>
              <th className="px-3 py-2 font-medium">Created</th>
            </tr>
          </thead>
          <tbody>
            {q.isPending ? (
              <tr>
                <td colSpan={6} className="px-3 py-6 text-muted">
                  Loading…
                </td>
              </tr>
            ) : rows.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-3 py-6 text-muted">
                  No sessions yet.{" "}
                  <Link className="text-primary underline underline-offset-2" to="/sessions/new">
                    Create one
                  </Link>
                  .
                </td>
              </tr>
            ) : (
              rows.map((s) => (
                <tr key={s.session_id} className="border-b border-border last:border-0">
                  <td className="px-3 py-2 font-mono text-xs">
                    <Link className="text-primary underline underline-offset-2" to={`/sessions/${s.session_id}`}>
                      {s.session_id.slice(0, 8)}…
                    </Link>
                  </td>
                  <td className="max-w-[200px] truncate px-3 py-2">{s.repo_url}</td>
                  <td className="px-3 py-2 font-mono text-xs">{s.ref}</td>
                  <td className="px-3 py-2">{s.workflow}</td>
                  <td className="px-3 py-2">{s.status}</td>
                  <td className="whitespace-nowrap px-3 py-2 text-muted">{s.created_at}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {q.hasNextPage ? (
        <button
          type="button"
          disabled={q.isFetchingNextPage}
          className="rounded-md border border-border bg-card px-4 py-2 text-sm font-medium disabled:opacity-50"
          onClick={() => void q.fetchNextPage()}
        >
          {q.isFetchingNextPage ? "Loading…" : "Load more"}
        </button>
      ) : null}
    </div>
  );
}
