import { controlPlaneJson } from "./client";
import type { Paginated, WorkerSummary } from "./types";

export function workersQueryString(opts: { limit?: number; cursor?: string | null }): string {
  const q = new URLSearchParams();
  if (opts.limit != null) q.set("limit", String(opts.limit));
  if (opts.cursor) q.set("cursor", opts.cursor);
  const s = q.toString();
  return s ? `?${s}` : "";
}

export async function listWorkersPage(
  baseUrl: string,
  apiKey: string,
  opts: { limit?: number; cursor?: string | null },
): Promise<Paginated<WorkerSummary>> {
  return controlPlaneJson<Paginated<WorkerSummary>>({
    baseUrl,
    path: `/workers${workersQueryString({ limit: opts.limit ?? 20, cursor: opts.cursor })}`,
    method: "GET",
    apiKey,
  });
}

/** Walks cursors until exhausted (for heterogeneity banner). */
export async function listAllWorkers(baseUrl: string, apiKey: string): Promise<WorkerSummary[]> {
  const out: WorkerSummary[] = [];
  let cursor: string | null | undefined;
  do {
    const page = await listWorkersPage(baseUrl, apiKey, { limit: 100, cursor });
    out.push(...page.items);
    cursor = page.next_cursor ?? null;
  } while (cursor);
  return out;
}
