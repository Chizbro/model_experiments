import type {
  ApiErrorBody,
  ApiKeyInfo,
  AuthStatus,
  CreateApiKeyResponse,
  CreateSessionRequest,
  CreateSessionResponse,
  HealthResponse,
  IdentityStatus,
  LogEntry,
  PaginatedResponse,
  PersonaSummary,
  Repository,
  SessionDetail,
  SessionSummary,
  Worker,
} from './types';

// ---- localStorage keys ----

const LS_URL_KEY = 'rh_control_plane_url';
const LS_API_KEY = 'rh_api_key';

// ---- Helpers ----

export function getControlPlaneUrl(): string {
  return (localStorage.getItem(LS_URL_KEY) ?? '').replace(/\/+$/, '');
}

export function setControlPlaneUrl(url: string): void {
  localStorage.setItem(LS_URL_KEY, url.replace(/\/+$/, ''));
}

export function getApiKey(): string {
  return localStorage.getItem(LS_API_KEY) ?? '';
}

export function setApiKey(key: string): void {
  localStorage.setItem(LS_API_KEY, key);
}

export function isConfigured(): boolean {
  return getControlPlaneUrl().length > 0 && getApiKey().length > 0;
}

export function clearCredentials(): void {
  localStorage.removeItem(LS_URL_KEY);
  localStorage.removeItem(LS_API_KEY);
}

// ---- Error classification ----

export type ErrorKind = 'cors' | 'network' | 'unauthorized' | 'client' | 'server' | 'unknown';

export class ApiError extends Error {
  kind: ErrorKind;
  status: number;
  code: string;
  details?: Record<string, unknown>;

  constructor(
    message: string,
    kind: ErrorKind,
    status: number = 0,
    code: string = '',
    details?: Record<string, unknown>,
  ) {
    super(message);
    this.name = 'ApiError';
    this.kind = kind;
    this.status = status;
    this.code = code;
    this.details = details;
  }
}

function classifyFetchError(_err: unknown): ApiError {
  // In the browser, TypeError from fetch is either CORS or network.
  // If the URL scheme is http/https and there is a TypeError, CORS is likely
  // when dev-tools show a preflight failure; but from JS alone we cannot
  // distinguish them reliably. We check if the error message hints at CORS.
  const msg = _err instanceof Error ? _err.message : String(_err);
  if (msg.includes('Failed to fetch') || msg.includes('NetworkError') || msg.includes('Load failed')) {
    // Heuristic: if the control plane URL is set and the origin differs, suggest CORS.
    const cpUrl = getControlPlaneUrl();
    const isCrossOrigin =
      cpUrl.length > 0 &&
      (() => {
        try {
          const cp = new URL(cpUrl);
          return cp.origin !== window.location.origin;
        } catch {
          return false;
        }
      })();
    if (isCrossOrigin) {
      return new ApiError(
        'Browser blocked the request (CORS). The admin must add this UI origin to CORS_ALLOWED_ORIGINS on the control plane.',
        'cors',
      );
    }
    return new ApiError(
      'Cannot reach the control plane at ' + (cpUrl || '(not configured)') + '. Check the URL and network.',
      'network',
    );
  }
  return new ApiError(msg, 'unknown');
}

async function parseErrorBody(res: Response): Promise<ApiError> {
  let body: ApiErrorBody | null = null;
  try {
    body = (await res.json()) as ApiErrorBody;
  } catch {
    // ignore parse failure
  }

  const code = body?.error?.code ?? '';
  const message = body?.error?.message ?? res.statusText;
  const details = body?.error?.details;

  if (res.status === 401) {
    return new ApiError(
      'Not authorized. ' + message,
      'unauthorized',
      401,
      code,
      details,
    );
  }
  if (res.status >= 400 && res.status < 500) {
    return new ApiError(message, 'client', res.status, code, details);
  }
  if (res.status >= 500) {
    return new ApiError(
      'Something went wrong on the server. ' + message,
      'server',
      res.status,
      code,
      details,
    );
  }
  return new ApiError(message, 'unknown', res.status, code, details);
}

// ---- Core fetch wrapper ----

async function apiFetch<T>(
  path: string,
  options: RequestInit = {},
  skipAuth = false,
): Promise<T> {
  const base = getControlPlaneUrl();
  if (!base) {
    throw new ApiError('Control plane URL not configured. Go to Settings.', 'network');
  }

  const headers: Record<string, string> = {
    ...(options.headers as Record<string, string> | undefined),
  };

  if (!skipAuth) {
    const key = getApiKey();
    if (!key) {
      throw new ApiError('API key not configured. Go to Settings.', 'unauthorized');
    }
    headers['Authorization'] = `Bearer ${key}`;
  }

  if (options.body && !headers['Content-Type']) {
    headers['Content-Type'] = 'application/json';
  }

  let res: Response;
  try {
    res = await fetch(`${base}${path}`, { ...options, headers });
  } catch (err) {
    throw classifyFetchError(err);
  }

  if (!res.ok) {
    throw await parseErrorBody(res);
  }

  // 204 No Content
  if (res.status === 204) {
    return undefined as unknown as T;
  }

  return (await res.json()) as T;
}

// ---- Public API functions ----

// Health (no auth)
export async function checkHealth(): Promise<HealthResponse> {
  return apiFetch<HealthResponse>('/health', {}, true);
}

// Sessions
export async function listSessions(params?: {
  status?: string;
  limit?: number;
  cursor?: string;
}): Promise<PaginatedResponse<SessionSummary>> {
  const qs = new URLSearchParams();
  if (params?.status) qs.set('status', params.status);
  if (params?.limit) qs.set('limit', String(params.limit));
  if (params?.cursor) qs.set('cursor', params.cursor);
  const q = qs.toString();
  return apiFetch<PaginatedResponse<SessionSummary>>(`/sessions${q ? '?' + q : ''}`);
}

export async function getSession(id: string): Promise<SessionDetail> {
  return apiFetch<SessionDetail>(`/sessions/${id}`);
}

export async function createSession(req: CreateSessionRequest): Promise<CreateSessionResponse> {
  return apiFetch<CreateSessionResponse>('/sessions', {
    method: 'POST',
    body: JSON.stringify(req),
  });
}

export async function deleteSession(id: string): Promise<void> {
  return apiFetch<void>(`/sessions/${id}`, { method: 'DELETE' });
}

export async function updateSession(
  id: string,
  body: { retain_forever: boolean },
): Promise<void> {
  return apiFetch<void>(`/sessions/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(body),
  });
}

export async function sendSessionInput(id: string, message: string): Promise<void> {
  return apiFetch<void>(`/sessions/${id}/input`, {
    method: 'POST',
    body: JSON.stringify({ message }),
  });
}

// Logs
export async function getLogHistory(
  sessionId: string,
  params?: { job_id?: string; limit?: number; cursor?: string; level?: string },
): Promise<PaginatedResponse<LogEntry>> {
  const qs = new URLSearchParams();
  if (params?.job_id) qs.set('job_id', params.job_id);
  if (params?.limit) qs.set('limit', String(params.limit));
  if (params?.cursor) qs.set('cursor', params.cursor);
  if (params?.level) qs.set('level', params.level);
  const q = qs.toString();
  return apiFetch<PaginatedResponse<LogEntry>>(`/sessions/${sessionId}/logs${q ? '?' + q : ''}`);
}

export async function deleteSessionLogs(
  sessionId: string,
  jobId?: string,
): Promise<void> {
  const qs = jobId ? `?job_id=${encodeURIComponent(jobId)}` : '';
  return apiFetch<void>(`/sessions/${sessionId}/logs${qs}`, { method: 'DELETE' });
}

// Workers
export async function listWorkers(): Promise<PaginatedResponse<Worker>> {
  return apiFetch<PaginatedResponse<Worker>>('/workers');
}

export async function deleteWorker(id: string): Promise<void> {
  return apiFetch<void>(`/workers/${id}`, { method: 'DELETE' });
}

// Identities
export async function getIdentity(id: string = 'default'): Promise<IdentityStatus> {
  return apiFetch<IdentityStatus>(`/identities/${id}`);
}

export async function getAuthStatus(id: string = 'default'): Promise<AuthStatus> {
  return apiFetch<AuthStatus>(`/identities/${id}/auth-status`);
}

export async function updateIdentity(
  id: string,
  body: { agent_token?: string; git_token?: string },
): Promise<void> {
  return apiFetch<void>(`/identities/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(body),
  });
}

export async function listRepositories(
  identityId: string = 'default',
): Promise<{ items: Repository[]; provider: string }> {
  return apiFetch<{ items: Repository[]; provider: string }>(
    `/identities/${identityId}/repositories`,
  );
}

// API keys
export async function bootstrapApiKey(
  label?: string,
): Promise<CreateApiKeyResponse> {
  return apiFetch<CreateApiKeyResponse>(
    '/api-keys/bootstrap',
    {
      method: 'POST',
      body: JSON.stringify({ label: label ?? 'web-ui-bootstrap' }),
    },
    true,
  );
}

export async function createApiKey(label?: string): Promise<CreateApiKeyResponse> {
  return apiFetch<CreateApiKeyResponse>('/api-keys', {
    method: 'POST',
    body: JSON.stringify({ label }),
  });
}

export async function listApiKeys(): Promise<PaginatedResponse<ApiKeyInfo>> {
  return apiFetch<PaginatedResponse<ApiKeyInfo>>('/api-keys');
}

export async function revokeApiKey(id: string): Promise<void> {
  return apiFetch<void>(`/api-keys/${id}`, { method: 'DELETE' });
}

// Personas
export async function listPersonas(): Promise<PaginatedResponse<PersonaSummary>> {
  return apiFetch<PaginatedResponse<PersonaSummary>>('/personas');
}
