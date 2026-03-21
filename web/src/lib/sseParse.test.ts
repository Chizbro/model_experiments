import { describe, expect, it } from "vitest";
import { SseLineBuffer } from "./sseParse";

describe("SseLineBuffer", () => {
  it("parses a log event across chunked reads", () => {
    const events: { ev?: string; data: string }[] = [];
    const buf = new SseLineBuffer();
    buf.push("event: log\nda", (ev, data) => events.push({ ev, data }));
    expect(events).toEqual([]);
    buf.push(`ta: {"id":"1"}\n\n`, (ev, data) => events.push({ ev, data }));
    expect(events).toEqual([{ ev: "log", data: '{"id":"1"}' }]);
  });

  it("ignores comment heartbeats", () => {
    const events: string[] = [];
    const buf = new SseLineBuffer();
    buf.push(": keep-alive\n\nevent: log\ndata: {}\n\n", (_ev, data) => events.push(data));
    expect(events).toEqual(["{}"]);
  });
});
