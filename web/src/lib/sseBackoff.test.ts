import { describe, expect, it } from "vitest";
import { nextBackoffMs } from "./sseBackoff";

describe("nextBackoffMs", () => {
  it("doubles until 30s cap", () => {
    expect(nextBackoffMs(0)).toBe(1000);
    expect(nextBackoffMs(1)).toBe(2000);
    expect(nextBackoffMs(5)).toBe(30_000);
    expect(nextBackoffMs(100)).toBe(30_000);
  });
});
