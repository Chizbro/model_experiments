import { describe, expect, it } from "vitest";
import { mapFetchFailure, mapHttpError } from "./errors";

describe("mapHttpError", () => {
  it("maps 401", () => {
    const m = mapHttpError(401, '{"error":{"code":"unauthorized","message":"bad key"}}');
    expect(m.title).toBe("Not authorized");
    expect(m.code).toBe("unauthorized");
  });

  it("maps 500", () => {
    const m = mapHttpError(500, "{}");
    expect(m.title).toBe("Server error");
  });
});

describe("mapFetchFailure", () => {
  it("suspects CORS on cross-origin Failed to fetch", () => {
    const { kind, mapped } = mapFetchFailure(new TypeError("Failed to fetch"), {
      baseUrl: "http://api.example:3000",
      uiOrigin: "http://localhost:5173",
    });
    expect(kind).toBe("cors_suspected");
    expect(mapped.title).toContain("CORS");
  });

  it("uses network copy for same-origin Failed to fetch", () => {
    const { kind, mapped } = mapFetchFailure(new TypeError("Failed to fetch"), {
      baseUrl: "http://localhost:3000",
      uiOrigin: "http://localhost:3000",
    });
    expect(kind).toBe("network");
    expect(mapped.title).toContain("reach");
  });
});
