import type { LogEntry } from "../api/types";

function sortKey(e: LogEntry): string {
  return `${e.timestamp}\0${e.id}`;
}

/** Merge two lists, dedupe by `id`, sort by timestamp then id. */
export function mergeAndSortLogs(existing: LogEntry[], incoming: LogEntry[]): LogEntry[] {
  const map = new Map<string, LogEntry>();
  for (const e of existing) {
    map.set(e.id, e);
  }
  for (const e of incoming) {
    map.set(e.id, e);
  }
  return Array.from(map.values()).sort((a, b) => sortKey(a).localeCompare(sortKey(b)));
}
