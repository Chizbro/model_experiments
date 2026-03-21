import { useEffect, useState } from "react";
import type { SessionSseLifecyclePayload } from "../api/types";
import { consumeSessionEventsSse } from "../lib/sseFetch";
import { nextBackoffMs } from "../lib/sseBackoff";

function isAbort(e: unknown): boolean {
  return e instanceof DOMException && e.name === "AbortError";
}

export function useSessionEventsStream(opts: {
  baseUrl: string;
  apiKey: string;
  sessionId: string;
  enabled: boolean;
}) {
  const [events, setEvents] = useState<SessionSseLifecyclePayload[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [reconnecting, setReconnecting] = useState(false);

  useEffect(() => {
    if (!opts.enabled || !opts.baseUrl.trim() || !opts.apiKey.trim() || !opts.sessionId.trim()) {
      setEvents([]);
      setError(null);
      setReconnecting(false);
      return;
    }

    setEvents([]);

    let cancelled = false;
    const ac = new AbortController();

    void (async () => {
      let attempt = 0;
      while (!cancelled) {
        try {
          setReconnecting(attempt > 0);
          setError(null);
          await consumeSessionEventsSse(
            opts.baseUrl,
            opts.apiKey,
            opts.sessionId,
            { signal: ac.signal },
            (ev) => {
              setEvents((prev) => [...prev, ev]);
            },
          );
          if (cancelled) break;
        } catch (e) {
          if (cancelled || isAbort(e)) break;
          setError(e instanceof Error ? e.message : String(e));
        }
        if (cancelled) break;
        await new Promise((r) => setTimeout(r, nextBackoffMs(attempt++)));
      }
      if (!cancelled) setReconnecting(false);
    })();

    return () => {
      cancelled = true;
      ac.abort();
    };
  }, [opts.enabled, opts.baseUrl, opts.apiKey, opts.sessionId]);

  return { events, error, reconnecting };
}
