// ---- Enums ----

export type SessionStatus = 'pending' | 'running' | 'completed' | 'failed';
export type JobStatus = 'pending' | 'assigned' | 'running' | 'completed' | 'failed';
export type WorkflowType = 'chat' | 'loop_n' | 'loop_until_sentinel' | 'inbox';
export type AgentCli = 'claude_code' | 'cursor';
export type BranchMode = 'main' | 'pr';
export type WorkerStatus = 'active' | 'stale';
export type LogLevel = 'debug' | 'info' | 'warn' | 'error';
export type GitTokenStatus =
  | 'healthy'
  | 'expiring_soon'
  | 'expired_refreshable'
  | 'expired_needs_reauth'
  | 'unknown'
  | 'not_configured';

// ---- Paginated response ----

export interface PaginatedResponse<T> {
  items: T[];
  next_cursor: string | null;
}

// ---- Sessions ----

export interface SessionSummary {
  session_id: string;
  repo_url: string;
  ref: string;
  workflow: WorkflowType;
  status: SessionStatus;
  created_at: string;
}

export interface Job {
  job_id: string;
  status: JobStatus;
  created_at: string;
  error_message: string | null;
  pull_request_url: string | null;
  branch: string | null;
  commit_ref: string | null;
  iteration_index?: number;
}

export interface SessionDetail {
  session_id: string;
  repo_url: string;
  ref: string;
  workflow: WorkflowType;
  status: SessionStatus;
  params: SessionParams;
  jobs: Job[];
  retain_forever: boolean;
  created_at: string;
  updated_at: string;
}

export interface SessionParams {
  prompt?: string;
  agent_cli?: AgentCli;
  n?: number;
  sentinel?: string;
  model?: string;
  branch_mode?: BranchMode;
  branch_name_prefix?: string;
  agent_id?: string;
}

export interface CreateSessionRequest {
  repo_url: string;
  ref?: string;
  workflow: WorkflowType;
  params: SessionParams;
  persona_id?: string;
  identity_id?: string;
  retain_forever?: boolean;
}

export interface CreateSessionResponse {
  session_id: string;
  status: SessionStatus;
  web_url?: string;
}

export interface SendInputRequest {
  message: string;
}

// ---- Workers ----

export interface Worker {
  worker_id: string;
  host: string;
  labels: Record<string, string>;
  status: WorkerStatus;
  last_seen_at: string;
}

// ---- Logs ----

export interface LogEntry {
  id: string;
  timestamp: string;
  level: LogLevel;
  session_id: string;
  job_id: string | null;
  worker_id: string | null;
  source: string;
  message: string;
}

// ---- Session Events (SSE) ----

export interface SessionEvent {
  event: 'started' | 'job_started' | 'job_completed' | 'completed' | 'failed';
  job_id?: string;
  payload?: Record<string, unknown>;
}

// ---- Identities ----

export interface IdentityStatus {
  has_git_token: boolean;
  has_agent_token: boolean;
}

export interface AuthStatus {
  git_token_status: GitTokenStatus;
  git_provider?: string;
  token_expires_at?: string;
  message?: string;
}

// ---- API keys ----

export interface ApiKeyInfo {
  id: string;
  label: string | null;
  created_at: string;
}

export interface CreateApiKeyResponse {
  id: string;
  key: string;
  label: string | null;
  created_at: string;
}

// ---- Personas ----

export interface PersonaSummary {
  persona_id: string;
  name: string;
}

export interface PersonaDetail {
  persona_id: string;
  name: string;
  prompt: string;
}

// ---- Repositories ----

export interface Repository {
  full_name: string;
  clone_url: string;
}

// ---- Health ----

export interface HealthResponse {
  status: string;
}

// ---- Error ----

export interface ApiErrorBody {
  error: {
    code: string;
    message: string;
    details?: Record<string, unknown>;
  };
}
