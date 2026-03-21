/** Standard control-plane error envelope ([API_OVERVIEW §2](../../docs/API_OVERVIEW.md)). */
export interface ApiErrorBody {
  error?: {
    code?: string;
    message?: string;
    details?: unknown;
  };
}

export type FetchFailureKind = "network" | "cors_suspected" | "tls_or_unknown";

export interface MappedApiError {
  title: string;
  detail: string;
  actionHint?: string;
  code?: string;
  httpStatus?: number;
}

export function parseApiErrorBody(text: string): ApiErrorBody | null {
  try {
    return JSON.parse(text) as ApiErrorBody;
  } catch {
    return null;
  }
}

/**
 * User-facing copy for REST failures ([CLIENT_EXPERIENCE §2.1, §3](../../docs/CLIENT_EXPERIENCE.md)).
 */
export function mapHttpError(status: number, bodyText: string): MappedApiError {
  const parsed = parseApiErrorBody(bodyText);
  const code = parsed?.error?.code;
  const message = parsed?.error?.message;

  if (status === 401 || code === "unauthorized") {
    return {
      title: "Not authorized",
      detail: message ?? "The server rejected this request (401).",
      actionHint: "Check Settings → API key.",
      code: code ?? "unauthorized",
      httpStatus: status,
    };
  }
  if (status === 400 || code === "invalid_request") {
    return {
      title: "Invalid request",
      detail: message ?? "The server could not process this request (400).",
      code: code ?? "invalid_request",
      httpStatus: status,
    };
  }
  if (status === 404 || code === "not_found") {
    return {
      title: "Not found",
      detail: message ?? "That resource does not exist (404).",
      code: code ?? "not_found",
      httpStatus: status,
    };
  }
  if (status === 409 || code === "conflict") {
    return {
      title: "Conflict",
      detail: message ?? "This action conflicts with the current state (409).",
      code: code ?? "conflict",
      httpStatus: status,
    };
  }
  if (status === 503 && code !== "not_ready") {
    return {
      title: "Service unavailable",
      detail: message ?? "Sign-in or another dependency may be misconfigured (503).",
      actionHint: "If this was OAuth: confirm provider env vars on the server.",
      code,
      httpStatus: status,
    };
  }
  if (status >= 500) {
    return {
      title: "Server error",
      detail: message ?? "Something went wrong on the server.",
      actionHint: "Retry; if it persists, check control-plane logs.",
      code,
      httpStatus: status,
    };
  }
  return {
    title: `HTTP ${status}`,
    detail: message ?? (bodyText.slice(0, 400) || "Unexpected response."),
    code,
    httpStatus: status,
  };
}

/**
 * When `fetch` throws (no response): distinguish unreachable vs likely CORS per CLIENT_EXPERIENCE §3.
 */
export function mapFetchFailure(
  err: unknown,
  opts: { baseUrl: string; uiOrigin: string },
): { kind: FetchFailureKind; mapped: MappedApiError } {
  const msg = err instanceof Error ? err.message : String(err);
  const ui = new URL(opts.uiOrigin);
  let apiHost: string;
  try {
    apiHost = new URL(opts.baseUrl).host;
  } catch {
    apiHost = "";
  }
  const crossOrigin = apiHost.length > 0 && apiHost !== ui.host;
  const looksLikeFailedFetch =
    err instanceof TypeError && (msg === "Failed to fetch" || msg.includes("NetworkError"));

  if (looksLikeFailedFetch && crossOrigin) {
    return {
      kind: "cors_suspected",
      mapped: {
        title: "Browser blocked the request (CORS)",
        detail:
          "The browser could not complete a cross-origin request to the control plane. This often means the UI origin is not listed in CORS_ALLOWED_ORIGINS on the server.",
        actionHint: "Ask your admin to add this UI origin to CORS_ALLOWED_ORIGINS. See docs/HOSTING.md and TROUBLESHOOTING §1a.",
      },
    };
  }

  if (looksLikeFailedFetch) {
    return {
      kind: "network",
      mapped: {
        title: "Cannot reach the control plane",
        detail: `No response from ${opts.baseUrl}. The host may be down, unreachable, or blocked.`,
        actionHint: "Confirm the URL, VPN/Tailscale, and that the server is running. Then retry.",
      },
    };
  }

  return {
    kind: "tls_or_unknown",
    mapped: {
      title: "Secure connection or network error",
      detail: msg || "The request failed before a response was received.",
      actionHint: "Check TLS certificates, DNS, and that the URL matches the certificate hostname.",
    },
  };
}
