import { controlPlaneFetch } from "./client";

export interface HealthPayload {
  status?: string;
  log_retention_days_default?: number;
  chat_history_max_turns?: number;
}

export async function fetchHealth(baseUrl: string): Promise<{
  ok: boolean;
  status: number;
  snippet: string;
  payload?: HealthPayload;
  /** Set when this is clearly the API (header from current servers; older binaries may omit it). */
  looksLikeControlPlane: boolean;
}> {
  const res = await controlPlaneFetch({ baseUrl, path: "/health", method: "GET" });
  const text = await res.text();
  let snippet = `${res.status}`;
  let payload: HealthPayload | undefined;
  try {
    const j = JSON.parse(text) as Record<string, unknown>;
    snippet = `${res.status} ${JSON.stringify(j)}`;
    payload = {
      status: typeof j.status === "string" ? j.status : undefined,
      log_retention_days_default:
        typeof j.log_retention_days_default === "number" ? j.log_retention_days_default : undefined,
      chat_history_max_turns:
        typeof j.chat_history_max_turns === "number" ? j.chat_history_max_turns : undefined,
    };
  } catch {
    if (text.length > 0 && text.length < 200) {
      snippet = `${res.status} ${text}`;
    }
  }
  const fingerprint = res.headers.get("x-remote-harness-control-plane");
  const looksLikeControlPlane =
    res.ok &&
    payload?.status === "ok" &&
    (fingerprint === "1" || fingerprint === null);
  return { ok: res.ok, status: res.status, snippet, payload, looksLikeControlPlane };
}
