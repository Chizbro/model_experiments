import { controlPlaneJson, controlPlaneNoContent } from "./client";
import type {
  CreateSessionBody,
  CreateSessionResponse,
  LogEntry,
  Paginated,
  SendSessionInputResponse,
  SessionDetail,
  SessionSummary,
} from "./types";

export function sessionsQueryString(opts: {
  limit?: number;
  cursor?: string | null;
  status?: string | null;
}): string {
  const q = new URLSearchParams();
  if (opts.limit != null) q.set("limit", String(opts.limit));
  if (opts.cursor) q.set("cursor", opts.cursor);
  if (opts.status?.trim()) q.set("status", opts.status.trim());
  const s = q.toString();
  return s ? `?${s}` : "";
}

export async function listSessionsPage(
  baseUrl: string,
  apiKey: string,
  opts: { limit?: number; cursor?: string | null; status?: string | null },
): Promise<Paginated<SessionSummary>> {
  return controlPlaneJson<Paginated<SessionSummary>>({
    baseUrl,
    path: `/sessions${sessionsQueryString({ limit: opts.limit ?? 20, cursor: opts.cursor, status: opts.status })}`,
    method: "GET",
    apiKey,
  });
}

export async function getSession(baseUrl: string, apiKey: string, sessionId: string): Promise<SessionDetail> {
  return controlPlaneJson<SessionDetail>({
    baseUrl,
    path: `/sessions/${encodeURIComponent(sessionId)}`,
    method: "GET",
    apiKey,
  });
}

export async function createSession(
  baseUrl: string,
  apiKey: string,
  body: CreateSessionBody,
): Promise<CreateSessionResponse> {
  return controlPlaneJson<CreateSessionResponse>({
    baseUrl,
    path: "/sessions",
    method: "POST",
    apiKey,
    jsonBody: body,
  });
}

function logsListQueryString(opts: {
  limit?: number;
  cursor?: string | null;
  jobId?: string | null;
  level?: string | null;
}): string {
  const q = new URLSearchParams();
  if (opts.limit != null) q.set("limit", String(opts.limit));
  if (opts.cursor) q.set("cursor", opts.cursor);
  if (opts.jobId?.trim()) q.set("job_id", opts.jobId.trim());
  if (opts.level?.trim()) q.set("level", opts.level.trim());
  const s = q.toString();
  return s ? `?${s}` : "";
}

/** Paginate until `next_cursor` is empty (API client contract, API_OVERVIEW §6). */
export async function fetchAllSessionLogs(
  baseUrl: string,
  apiKey: string,
  sessionId: string,
  opts?: { jobId?: string | null; level?: string | null; signal?: AbortSignal },
): Promise<LogEntry[]> {
  const sid = encodeURIComponent(sessionId);
  const all: LogEntry[] = [];
  let cursor: string | null = null;
  for (;;) {
    const qs = logsListQueryString({
      limit: 100,
      cursor,
      jobId: opts?.jobId,
      level: opts?.level,
    });
    const page = await controlPlaneJson<Paginated<LogEntry>>({
      baseUrl,
      path: `/sessions/${sid}/logs${qs}`,
      method: "GET",
      apiKey,
      signal: opts?.signal,
    });
    all.push(...page.items);
    const next = page.next_cursor?.trim();
    if (!next) break;
    cursor = next;
  }
  return all;
}

export async function deleteSessionLogs(
  baseUrl: string,
  apiKey: string,
  sessionId: string,
  jobId?: string | null,
): Promise<void> {
  const sid = encodeURIComponent(sessionId);
  const q = jobId?.trim() ? `?job_id=${encodeURIComponent(jobId.trim())}` : "";
  await controlPlaneNoContent({
    baseUrl,
    path: `/sessions/${sid}/logs${q}`,
    method: "DELETE",
    apiKey,
  });
}

export async function patchSessionRetain(
  baseUrl: string,
  apiKey: string,
  sessionId: string,
  retainForever: boolean,
): Promise<void> {
  const sid = encodeURIComponent(sessionId);
  await controlPlaneNoContent({
    baseUrl,
    path: `/sessions/${sid}`,
    method: "PATCH",
    apiKey,
    jsonBody: { retain_forever: retainForever },
  });
}

export async function patchJobRetain(
  baseUrl: string,
  apiKey: string,
  sessionId: string,
  jobId: string,
  retainForever: boolean,
): Promise<void> {
  const sid = encodeURIComponent(sessionId);
  const jid = encodeURIComponent(jobId);
  await controlPlaneNoContent({
    baseUrl,
    path: `/sessions/${sid}/jobs/${jid}`,
    method: "PATCH",
    apiKey,
    jsonBody: { retain_forever: retainForever },
  });
}

export async function sendSessionInput(
  baseUrl: string,
  apiKey: string,
  sessionId: string,
  message: string,
): Promise<SendSessionInputResponse> {
  const sid = encodeURIComponent(sessionId);
  return controlPlaneJson<SendSessionInputResponse>({
    baseUrl,
    path: `/sessions/${sid}/input`,
    method: "POST",
    apiKey,
    jsonBody: { message },
  });
}
