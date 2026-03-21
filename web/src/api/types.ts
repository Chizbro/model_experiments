export interface Paginated<T> {
  items: T[];
  next_cursor?: string | null;
}

export interface SessionSummary {
  session_id: string;
  repo_url: string;
  ref: string;
  workflow: string;
  status: string;
  created_at: string;
}

export interface SessionJobDetail {
  job_id: string;
  status: string;
  created_at: string;
  error_message?: string | null;
  pull_request_url?: string | null;
  commit_ref?: string | null;
  retain_forever?: boolean;
}

export interface SessionDetail {
  session_id: string;
  repo_url: string;
  ref: string;
  workflow: string;
  status: string;
  params: Record<string, unknown>;
  jobs: SessionJobDetail[];
  created_at: string;
  updated_at: string;
  retain_forever?: boolean;
  /** Chat: next pull would cap history (see API_OVERVIEW — Get session). */
  chat_history_truncated?: boolean;
  chat_history_max_turns?: number | null;
}

export interface LogEntry {
  id: string;
  timestamp: string;
  level: string;
  session_id: string;
  job_id?: string | null;
  worker_id?: string | null;
  source: string;
  message: string;
}

/** `event: session_event` data JSON (see repo `docs/SSE_EVENTS.md`). */
export interface SessionSseLifecyclePayload {
  event: string;
  job_id?: string;
  payload: Record<string, unknown>;
}

export interface SendSessionInputResponse {
  accepted: boolean;
}

export interface CreateSessionBody {
  repo_url: string;
  ref?: string;
  workflow: string;
  params: Record<string, unknown>;
  persona_id?: string;
  identity_id?: string;
  retain_forever?: boolean;
}

export interface CreateSessionResponse {
  session_id: string;
  status: string;
  web_url?: string;
}

export interface WorkerSummary {
  worker_id: string;
  host?: string | null;
  labels: Record<string, unknown>;
  status: string;
  last_seen_at?: string | null;
  capabilities?: string[] | null;
}

export interface IdentityCredentials {
  has_git_token: boolean;
  has_agent_token: boolean;
}
