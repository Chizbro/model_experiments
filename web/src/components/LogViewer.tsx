import { useState, useEffect, useRef, useCallback } from 'react';
import { getLogHistory } from '../api/client';
import { connectSSE } from '../api/sse';
import type { LogEntry } from '../api/types';
import type { SSEConnection } from '../api/sse';

interface LogViewerProps {
  sessionId: string;
  jobId?: string;
  /** When true, the session is in a terminal state and SSE will not be opened. */
  sessionEnded?: boolean;
}

const LOG_LEVEL_COLORS: Record<string, string> = {
  debug: 'text-gray-400',
  info: 'text-blue-600',
  warn: 'text-yellow-600',
  error: 'text-red-600',
};

const LOG_LEVEL_BG: Record<string, string> = {
  debug: 'bg-gray-100 text-gray-600',
  info: 'bg-blue-100 text-blue-700',
  warn: 'bg-yellow-100 text-yellow-700',
  error: 'bg-red-100 text-red-700',
};

export default function LogViewer({ sessionId, jobId, sessionEnded }: LogViewerProps) {
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [historyLoaded, setHistoryLoaded] = useState(false);
  const [reconnecting, setReconnecting] = useState(false);
  const [showTimestampRelative, setShowTimestampRelative] = useState(true);
  const [autoScroll, setAutoScroll] = useState(true);
  const [newLogsAvailable, setNewLogsAvailable] = useState(false);

  const containerRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const sseRef = useRef<SSEConnection | null>(null);
  const seenIdsRef = useRef<Set<string>>(new Set());

  // Scroll tracking
  const handleScroll = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 50;
    setAutoScroll(atBottom);
    if (atBottom) {
      setNewLogsAvailable(false);
    }
  }, []);

  // Scroll to bottom
  const scrollToBottom = useCallback(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
    setAutoScroll(true);
    setNewLogsAvailable(false);
  }, []);

  // Auto-scroll effect
  useEffect(() => {
    if (autoScroll) {
      bottomRef.current?.scrollIntoView({ behavior: 'auto' });
    }
  }, [entries, autoScroll]);

  // Load full history by paginating
  useEffect(() => {
    let cancelled = false;

    async function loadHistory() {
      setLoading(true);
      const allEntries: LogEntry[] = [];
      let cursor: string | undefined;

      try {
        let hasMore = true;
        while (hasMore) {
          const page = await getLogHistory(sessionId, {
            job_id: jobId,
            limit: 100,
            cursor,
          });
          if (cancelled) return;

          for (const entry of page.items) {
            if (!seenIdsRef.current.has(entry.id)) {
              seenIdsRef.current.add(entry.id);
              allEntries.push(entry);
            }
          }

          if (!page.next_cursor) {
            hasMore = false;
          } else {
            cursor = page.next_cursor;
          }
        }

        if (!cancelled) {
          setEntries(allEntries);
          setHistoryLoaded(true);
          setLoading(false);
        }
      } catch (err) {
        if (!cancelled) {
          console.error('Failed to load log history:', err);
          setLoading(false);
          setHistoryLoaded(true);
        }
      }
    }

    loadHistory();
    return () => {
      cancelled = true;
    };
  }, [sessionId, jobId]);

  // Start SSE stream after history is loaded
  useEffect(() => {
    if (!historyLoaded || sessionEnded) return;

    const qs = jobId ? `?job_id=${encodeURIComponent(jobId)}` : '';
    const conn = connectSSE(`/sessions/${sessionId}/logs/stream${qs}`, {
      onEvent(eventType, data) {
        if (eventType === 'log') {
          try {
            const entry = JSON.parse(data) as LogEntry;
            if (!seenIdsRef.current.has(entry.id)) {
              seenIdsRef.current.add(entry.id);
              setEntries((prev) => [...prev, entry]);
              if (!autoScroll) {
                setNewLogsAvailable(true);
              }
            }
          } catch {
            // Ignore malformed events
          }
        }
      },
      onReconnecting() {
        setReconnecting(true);
      },
      onOpen() {
        setReconnecting(false);
      },
      onError(err) {
        console.error('SSE error:', err);
      },
    });

    sseRef.current = conn;
    return () => {
      conn.close();
      sseRef.current = null;
    };
    // We intentionally do NOT add autoScroll to the dependency array,
    // because we don't want to reconnect the SSE when scroll state changes.
    // The onEvent callback captures autoScroll via the setNewLogsAvailable path
    // which reads autoScroll at call time, not at closure creation. We use a
    // stable pattern: setEntries(prev => ...) does not need the outer autoScroll.
    // The "new logs" flag is set unconditionally and the render reads autoScroll.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [historyLoaded, sessionEnded, sessionId, jobId]);

  // Close SSE on unmount
  useEffect(() => {
    return () => {
      sseRef.current?.close();
    };
  }, []);

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div className="flex items-center justify-between border-b bg-gray-50 px-3 py-1.5">
        <div className="flex items-center gap-3">
          <span className="text-xs font-medium text-gray-500 uppercase">Logs</span>
          {reconnecting && (
            <span className="inline-flex items-center gap-1 text-xs text-yellow-600">
              <span className="h-2 w-2 animate-pulse rounded-full bg-yellow-500" />
              Reconnecting...
            </span>
          )}
        </div>
        <button
          onClick={() => setShowTimestampRelative(!showTimestampRelative)}
          className="text-xs text-gray-500 hover:text-gray-700"
        >
          {showTimestampRelative ? 'Absolute time' : 'Relative time'}
        </button>
      </div>

      {/* Log entries */}
      <div
        ref={containerRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto bg-gray-900 p-2 font-mono text-xs leading-5"
      >
        {loading && (
          <div className="flex items-center justify-center py-8">
            <div className="h-6 w-6 animate-spin rounded-full border-2 border-blue-400 border-t-transparent" />
            <span className="ml-2 text-gray-400">Loading log history...</span>
          </div>
        )}

        {!loading && entries.length === 0 && (
          <div className="py-8 text-center text-gray-500">
            No log entries yet.
          </div>
        )}

        {entries.map((entry) => (
          <div key={entry.id} className="flex gap-2 py-0.5 hover:bg-gray-800/50">
            <span className="shrink-0 text-gray-500 select-none">
              {showTimestampRelative
                ? formatRelativeTimestamp(entry.timestamp)
                : formatAbsoluteTimestamp(entry.timestamp)}
            </span>
            <span
              className={`shrink-0 rounded px-1 text-center uppercase ${
                LOG_LEVEL_BG[entry.level] ?? LOG_LEVEL_BG.info
              }`}
              style={{ minWidth: '3rem' }}
            >
              {entry.level}
            </span>
            {entry.source && (
              <span className="shrink-0 text-gray-500">[{entry.source}]</span>
            )}
            <span className={LOG_LEVEL_COLORS[entry.level] ?? 'text-gray-300'}>
              {entry.message}
            </span>
          </div>
        ))}

        <div ref={bottomRef} />
      </div>

      {/* New logs button */}
      {newLogsAvailable && !autoScroll && (
        <button
          onClick={scrollToBottom}
          className="absolute bottom-16 left-1/2 -translate-x-1/2 rounded-full bg-blue-600 px-4 py-1.5 text-xs font-medium text-white shadow-lg hover:bg-blue-700 transition-all"
        >
          New logs -- scroll to bottom
        </button>
      )}
    </div>
  );
}

function formatRelativeTimestamp(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ago`;
}

function formatAbsoluteTimestamp(iso: string): string {
  const d = new Date(iso);
  const hms = d.toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
  const ms = String(d.getMilliseconds()).padStart(3, '0');
  return `${hms}.${ms}`;
}
