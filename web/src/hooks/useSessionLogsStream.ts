import { useEffect, useState } from "react";
import { fetchAllSessionLogs } from "../api/sessions";
import type { LogEntry } from "../api/types";
import { mergeAndSortLogs } from "../lib/logMerge";
import { consumeLogsSse } from "../lib/sseFetch";
import { nextBackoffMs } from "../lib/sseBackoff";

function isAbort(e: unknown): boolean {
  return e instanceof DOMException && e.name === "AbortError";
}

export interface UseSessionLogsStreamOptions {
  baseUrl: string;
  apiKey: string;
  sessionId: string;
  jobId?: string | null;
  level?: string | null;
  enabled: boolean;
  /** Increment to reload history and restart SSE (e.g. after DELETE logs). */
  refreshKey?: number;
}

export function useSessionLogsStream(opts: UseSessionLogsStreamOptions) {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [loadingHistory, setLoadingHistory] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [streamError, setStreamError] = useState<string | null>(null);
  const [reconnecting, setReconnecting] = useState(false);

  const refreshKey = opts.refreshKey ?? 0;
  const jobKey = opts.jobId ?? "";
  const levelKey = opts.level ?? "";

  useEffect(() => {
    if (!opts.enabled || !opts.baseUrl.trim() || !opts.apiKey.trim() || !opts.sessionId.trim()) {
      setLogs([]);
      setLoadingHistory(false);
      setHistoryError(null);
      setStreamError(null);
      setReconnecting(false);
      return;
    }

    let cancelled = false;
    const ac = new AbortController();
    const seen = new Set<string>();

    void (async () => {
      try {
        setHistoryError(null);
        setStreamError(null);
        setLoadingHistory(true);
        const initial = await fetchAllSessionLogs(opts.baseUrl, opts.apiKey, opts.sessionId, {
          jobId: jobKey || null,
          level: levelKey || null,
          signal: ac.signal,
        });
        if (cancelled) return;
        const sorted = mergeAndSortLogs([], initial);
        sorted.forEach((e) => seen.add(e.id));
        setLogs(sorted);
      } catch (e) {
        if (cancelled || isAbort(e)) return;
        setHistoryError(e instanceof Error ? e.message : String(e));
        setLoadingHistory(false);
        return;
      }
      setLoadingHistory(false);

      let attempt = 0;
      while (!cancelled) {
        try {
          setReconnecting(attempt > 0);
          setStreamError(null);
          await consumeLogsSse(
            opts.baseUrl,
            opts.apiKey,
            opts.sessionId,
            { jobId: jobKey || null, level: levelKey || null, signal: ac.signal },
            (entry) => {
              if (seen.has(entry.id)) return;
              seen.add(entry.id);
              setLogs((prev) => mergeAndSortLogs(prev, [entry]));
            },
          );
          if (cancelled) break;
        } catch (e) {
          if (cancelled || isAbort(e)) break;
          setStreamError(e instanceof Error ? e.message : String(e));
        }
        if (cancelled) break;
        const delay = nextBackoffMs(attempt++);
        setReconnecting(true);
        await new Promise((r) => setTimeout(r, delay));
        if (cancelled) break;
        try {
          const refill = await fetchAllSessionLogs(opts.baseUrl, opts.apiKey, opts.sessionId, {
            jobId: jobKey || null,
            level: levelKey || null,
            signal: ac.signal,
          });
          if (cancelled) return;
          setLogs((prev) => {
            const merged = mergeAndSortLogs(prev, refill);
            seen.clear();
            merged.forEach((x) => seen.add(x.id));
            return merged;
          });
        } catch (e) {
          if (cancelled || isAbort(e)) break;
        }
      }
      if (!cancelled) setReconnecting(false);
    })();

    return () => {
      cancelled = true;
      ac.abort();
    };
  }, [opts.enabled, opts.baseUrl, opts.apiKey, opts.sessionId, jobKey, levelKey, refreshKey]);

  return { logs, loadingHistory, historyError, streamError, reconnecting };
}
