import { mapFetchFailure, mapHttpError, type MappedApiError } from "./errors";

export class ControlPlaneHttpError extends Error {
  readonly mapped: MappedApiError;
  readonly status: number;
  readonly bodyText: string;

  constructor(status: number, bodyText: string) {
    const mapped = mapHttpError(status, bodyText);
    super(mapped.detail);
    this.name = "ControlPlaneHttpError";
    this.mapped = mapped;
    this.status = status;
    this.bodyText = bodyText;
  }
}

export interface JsonFetchOptions extends Omit<RequestInit, "body"> {
  baseUrl: string;
  path: string;
  /** When set, sends Authorization: Bearer … */
  apiKey?: string;
  jsonBody?: unknown;
}

/**
 * `fetch` to the control plane with optional JSON body and bearer auth.
 * Throws ControlPlaneHttpError on non-OK JSON error responses.
 */
export async function controlPlaneFetch(opts: JsonFetchOptions): Promise<Response> {
  const { baseUrl, path, apiKey, jsonBody, headers: hdrs, ...rest } = opts;
  const url = `${baseUrl.replace(/\/$/, "")}${path.startsWith("/") ? path : `/${path}`}`;
  const headers = new Headers(hdrs);
  if (jsonBody !== undefined) {
    headers.set("Content-Type", "application/json");
  }
  if (apiKey?.trim()) {
    headers.set("Authorization", `Bearer ${apiKey.trim()}`);
  }

  let res: Response;
  try {
    res = await fetch(url, {
      ...rest,
      headers,
      body: jsonBody === undefined ? undefined : JSON.stringify(jsonBody),
    });
  } catch (e) {
    const { mapped } = mapFetchFailure(e, {
      baseUrl,
      uiOrigin: typeof window !== "undefined" ? window.location.origin : "http://localhost",
    });
    const w = new Error(mapped.detail) as Error & { mapped?: MappedApiError };
    w.name = "ControlPlaneNetworkError";
    w.mapped = mapped;
    throw w;
  }

  return res;
}

export async function controlPlaneJson<T>(opts: JsonFetchOptions): Promise<T> {
  const res = await controlPlaneFetch(opts);
  const text = await res.text();
  if (!res.ok) {
    throw new ControlPlaneHttpError(res.status, text);
  }
  if (!text.length) {
    return undefined as T;
  }
  return JSON.parse(text) as T;
}

/** For `204 No Content` responses (e.g. PATCH / DELETE). */
export async function controlPlaneNoContent(opts: JsonFetchOptions): Promise<void> {
  const res = await controlPlaneFetch(opts);
  if (!res.ok) {
    const text = await res.text();
    throw new ControlPlaneHttpError(res.status, text);
  }
}
