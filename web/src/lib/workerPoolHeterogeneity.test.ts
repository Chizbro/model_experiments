import { describe, expect, it } from "vitest";
import {
  canonicalPlatformKey,
  isWorkerPoolHeterogeneous,
  shouldConfirmAgentCliAgainstPool,
} from "./workerPoolHeterogeneity";

describe("canonicalPlatformKey", () => {
  it("merges Windows spellings", () => {
    expect(canonicalPlatformKey({ platform: "win32" })).toBe("__windows_native__");
    expect(canonicalPlatformKey({ platform: "Windows" })).toBe("__windows_native__");
  });

  it("keeps WSL distinct", () => {
    expect(canonicalPlatformKey({ platform: "wsl" })).toBe("wsl");
  });
});

describe("isWorkerPoolHeterogeneous", () => {
  it("is false with fewer than two active workers", () => {
    expect(
      isWorkerPoolHeterogeneous([
        { status: "active", labels: { platform: "linux" } },
        { status: "stale", labels: { platform: "darwin" } },
      ]),
    ).toBe(false);
  });

  it("is false when active workers share the same canonical platform", () => {
    expect(
      isWorkerPoolHeterogeneous([
        { status: "active", labels: { platform: "linux" } },
        { status: "active", labels: { platform: "linux" } },
      ]),
    ).toBe(false);
  });

  it("is true when active workers differ on labels.platform", () => {
    expect(
      isWorkerPoolHeterogeneous([
        { status: "active", labels: { platform: "linux" } },
        { status: "active", labels: { platform: "darwin" } },
      ]),
    ).toBe(true);
  });

  it("is true for WSL vs native Windows mix", () => {
    expect(
      isWorkerPoolHeterogeneous([
        { status: "active", labels: { platform: "wsl" } },
        { status: "active", labels: { platform: "windows" } },
      ]),
    ).toBe(true);
  });

  it("is false when platforms are missing (cannot compare)", () => {
    expect(
      isWorkerPoolHeterogeneous([
        { status: "active", labels: {} },
        { status: "active", labels: {} },
      ]),
    ).toBe(false);
  });
});

describe("shouldConfirmAgentCliAgainstPool", () => {
  it("is true when there are no active workers", () => {
    expect(shouldConfirmAgentCliAgainstPool([{ status: "stale", labels: { platform: "linux" } }])).toBe(true);
  });

  it("is true when the pool is heterogeneous", () => {
    expect(
      shouldConfirmAgentCliAgainstPool([
        { status: "active", labels: { platform: "linux" } },
        { status: "active", labels: { platform: "darwin" } },
      ]),
    ).toBe(true);
  });

  it("is false for a single homogeneous active pool", () => {
    expect(
      shouldConfirmAgentCliAgainstPool([{ status: "active", labels: { platform: "linux" } }]),
    ).toBe(false);
  });
});
