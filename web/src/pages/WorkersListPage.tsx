import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { listAllWorkers, listWorkersPage } from "../api/workers";
import type { WorkerSummary } from "../api/types";
import { useSettings } from "../hooks/useSettings";
import { ControlPlaneHttpError } from "../api/client";
import { useMemo } from "react";
import { WorkerPoolHeterogeneityBanner } from "../components/WorkerPoolHeterogeneityBanner";
import { isWorkerPoolHeterogeneous } from "../lib/workerPoolHeterogeneity";

export function WorkersListPage() {
  const { controlPlaneUrl, apiKey } = useSettings();
  const base = controlPlaneUrl!;
  const key = apiKey.trim();

  const allWorkersQuery = useQuery({
    queryKey: ["workers-all-heterogeneity", base, key],
    queryFn: () => listAllWorkers(base, key),
    enabled: Boolean(base && key),
  });

  const pageQuery = useInfiniteQuery({
    queryKey: ["workers-page", base, key],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => listWorkersPage(base, key, { limit: 20, cursor: pageParam }),
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    enabled: Boolean(base && key),
  });

  const rows: WorkerSummary[] = useMemo(() => pageQuery.data?.pages.flatMap((p) => p.items) ?? [], [pageQuery.data]);

  const heterogeneous = allWorkersQuery.data ? isWorkerPoolHeterogeneous(allWorkersQuery.data) : false;

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Workers</h1>
        <p className="mt-1 max-w-2xl text-sm text-muted">GET /workers with cursor pagination. Pool mix is evaluated across all registered rows.</p>
      </div>

      {allWorkersQuery.isError ? (
        <p className="text-sm text-destructive">
          {allWorkersQuery.error instanceof ControlPlaneHttpError
            ? `${allWorkersQuery.error.mapped.title}: ${allWorkersQuery.error.mapped.detail}`
            : (allWorkersQuery.error as Error).message}
        </p>
      ) : (
        <WorkerPoolHeterogeneityBanner show={heterogeneous} />
      )}

      {pageQuery.isError ? (
        <p className="text-sm text-destructive">
          {pageQuery.error instanceof ControlPlaneHttpError
            ? `${pageQuery.error.mapped.title}: ${pageQuery.error.mapped.detail}`
            : (pageQuery.error as Error).message}
        </p>
      ) : null}

      <div className="overflow-x-auto rounded-lg border border-border bg-card shadow-sm">
        <table className="w-full min-w-[640px] border-collapse text-left text-sm">
          <thead className="border-b border-border bg-black/[0.02] text-xs uppercase tracking-wide text-muted">
            <tr>
              <th className="px-3 py-2 font-medium">Worker</th>
              <th className="px-3 py-2 font-medium">Status</th>
              <th className="px-3 py-2 font-medium">Host</th>
              <th className="px-3 py-2 font-medium">Labels</th>
              <th className="px-3 py-2 font-medium">Last seen</th>
            </tr>
          </thead>
          <tbody>
            {pageQuery.isPending ? (
              <tr>
                <td colSpan={5} className="px-3 py-6 text-muted">
                  Loading…
                </td>
              </tr>
            ) : rows.length === 0 ? (
              <tr>
                <td colSpan={5} className="px-3 py-6 text-muted">
                  No workers registered.
                </td>
              </tr>
            ) : (
              rows.map((w) => (
                <tr key={w.worker_id} className="border-b border-border last:border-0">
                  <td className="px-3 py-2 font-mono text-xs">{w.worker_id}</td>
                  <td className="px-3 py-2">{w.status}</td>
                  <td className="px-3 py-2 font-mono text-xs">{w.host ?? "—"}</td>
                  <td className="max-w-[280px] truncate px-3 py-2 font-mono text-xs">
                    {JSON.stringify(w.labels ?? {})}
                  </td>
                  <td className="whitespace-nowrap px-3 py-2 text-muted">{w.last_seen_at ?? "—"}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {pageQuery.hasNextPage ? (
        <button
          type="button"
          disabled={pageQuery.isFetchingNextPage}
          className="rounded-md border border-border bg-card px-4 py-2 text-sm font-medium disabled:opacity-50"
          onClick={() => void pageQuery.fetchNextPage()}
        >
          {pageQuery.isFetchingNextPage ? "Loading…" : "Load more"}
        </button>
      ) : null}
    </div>
  );
}
