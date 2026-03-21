import { describe, expect, it } from "vitest";
import type { LogEntry } from "../api/types";
import { mergeAndSortLogs } from "./logMerge";

function entry(id: string, ts: string, msg: string): LogEntry {
  return {
    id,
    timestamp: ts,
    level: "info",
    session_id: "s",
    source: "worker",
    message: msg,
  };
}

describe("mergeAndSortLogs", () => {
  it("dedupes by id and sorts by timestamp then id", () => {
    const a = entry("b", "2025-01-02T00:00:00Z", "second");
    const b = entry("a", "2025-01-01T00:00:00Z", "first");
    const c = entry("b", "2025-01-02T00:00:00Z", "updated");
    const out = mergeAndSortLogs([a], [b, c]);
    expect(out.map((e) => e.id)).toEqual(["a", "b"]);
    expect(out[1]!.message).toBe("updated");
  });

  it("merges reconnect refetch without duplicate rows (acceptance: no duplicate history)", () => {
    const historical = [entry("1", "t1", "a"), entry("2", "t2", "b")];
    const refetch = [entry("1", "t1", "a"), entry("2", "t2", "b"), entry("3", "t3", "c")];
    const merged = mergeAndSortLogs(historical, refetch);
    expect(merged).toHaveLength(3);
  });
});
