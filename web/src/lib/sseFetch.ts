import { ControlPlaneHttpError } from "../api/client";
import type { LogEntry, SessionSseLifecyclePayload } from "../api/types";
import { SseLineBuffer } from "./sseParse";

function streamUrl(
  baseUrl: string,
  pathWithQuery: string,
): string {
  const root = baseUrl.replace(/\/$/, "");
  const p = pathWithQuery.startsWith("/") ? pathWithQuery : `/${pathWithQuery}`;
  return `${root}${p}`;
}

export function logsStreamPath(sessionId: string, jobId?: string | null, level?: string | null): string {
  const sid = encodeURIComponent(sessionId);
  const q = new URLSearchParams();
  if (jobId?.trim()) q.set("job_id", jobId.trim());
  if (level?.trim()) q.set("level", level.trim());
  const qs = q.toString();
  return `/sessions/${sid}/logs/stream${qs ? `?${qs}` : ""}`;
}

export function sessionEventsStreamPath(sessionId: string): string {
  const sid = encodeURIComponent(sessionId);
  return `/sessions/${sid}/events`;
}

export async function consumeLogsSse(
  baseUrl: string,
  apiKey: string,
  sessionId: string,
  opts: { jobId?: string | null; level?: string | null; signal?: AbortSignal },
  onLog: (e: LogEntry) => void,
): Promise<void> {
  const url = streamUrl(baseUrl, logsStreamPath(sessionId, opts.jobId, opts.level));
  const res = await fetch(url, {
    headers: { Authorization: `Bearer ${apiKey.trim()}` },
    signal: opts.signal,
  });
  if (!res.ok) {
    const text = await res.text();
    throw new ControlPlaneHttpError(res.status, text);
  }
  if (!res.body) {
    throw new Error("Log stream response has no body");
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  const parser = new SseLineBuffer();
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      parser.push(decoder.decode(value, { stream: true }), (ev, data) => {
        if (ev !== "log" || !data.trim()) return;
        try {
          onLog(JSON.parse(data) as LogEntry);
        } catch {
          /* ignore malformed */
        }
      });
    }
  } finally {
    reader.releaseLock();
  }
}

export async function consumeSessionEventsSse(
  baseUrl: string,
  apiKey: string,
  sessionId: string,
  opts: { signal?: AbortSignal },
  onEvent: (e: SessionSseLifecyclePayload) => void,
): Promise<void> {
  const url = streamUrl(baseUrl, sessionEventsStreamPath(sessionId));
  const res = await fetch(url, {
    headers: { Authorization: `Bearer ${apiKey.trim()}` },
    signal: opts.signal,
  });
  if (!res.ok) {
    const text = await res.text();
    throw new ControlPlaneHttpError(res.status, text);
  }
  if (!res.body) {
    throw new Error("Session events stream response has no body");
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  const parser = new SseLineBuffer();
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      parser.push(decoder.decode(value, { stream: true }), (ev, data) => {
        if (ev !== "session_event" || !data.trim()) return;
        try {
          onEvent(JSON.parse(data) as SessionSseLifecyclePayload);
        } catch {
          /* ignore */
        }
      });
    }
  } finally {
    reader.releaseLock();
  }
}
