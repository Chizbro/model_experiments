const URL_KEY = "rh_control_plane_url";
const API_KEY_KEY = "rh_api_key";
const WAKE_URL_KEY = "rh_wake_url";
const GIT_IDENTITY_ID_KEY = "rh_git_identity_id";
const BOOTSTRAP_PREFIX = "rh_bootstrap_ineligible:";

export function normalizeControlPlaneUrl(raw: string): string {
  return raw.trim().replace(/\/+$/, "");
}

export function readStoredControlPlaneUrl(): string | null {
  const v = localStorage.getItem(URL_KEY);
  return v && v.trim().length > 0 ? normalizeControlPlaneUrl(v) : null;
}

export function writeStoredControlPlaneUrl(url: string): void {
  localStorage.setItem(URL_KEY, normalizeControlPlaneUrl(url));
}

export function readStoredApiKey(): string {
  return localStorage.getItem(API_KEY_KEY) ?? "";
}

export function writeStoredApiKey(key: string): void {
  localStorage.setItem(API_KEY_KEY, key);
}

export function readStoredWakeUrl(): string {
  return localStorage.getItem(WAKE_URL_KEY) ?? "";
}

export function writeStoredWakeUrl(url: string): void {
  localStorage.setItem(WAKE_URL_KEY, url.trim());
}

export function bootstrapIneligibleKey(baseUrl: string): string {
  return `${BOOTSTRAP_PREFIX}${normalizeControlPlaneUrl(baseUrl)}`;
}

export function readBootstrapIneligible(baseUrl: string): boolean {
  return localStorage.getItem(bootstrapIneligibleKey(baseUrl)) === "1";
}

export function writeBootstrapIneligible(baseUrl: string, value: boolean): void {
  const k = bootstrapIneligibleKey(baseUrl);
  if (value) {
    localStorage.setItem(k, "1");
  } else {
    localStorage.removeItem(k);
  }
}

/** Default dev URL; Vite env can pre-fill Settings UI only (not auto-persisted). */
export function envSuggestedControlPlaneUrl(): string {
  const env = import.meta.env.VITE_CONTROL_PLANE_URL?.trim();
  return env ? normalizeControlPlaneUrl(env) : "http://127.0.0.1:3000";
}

/** Identity id used for Git OAuth and BYOL API paths (`default` if unset). */
export function readStoredGitIdentityId(): string {
  const v = localStorage.getItem(GIT_IDENTITY_ID_KEY)?.trim();
  return v && v.length > 0 ? v : "default";
}

export function writeStoredGitIdentityId(id: string): void {
  const t = id.trim();
  localStorage.setItem(GIT_IDENTITY_ID_KEY, t.length > 0 ? t : "default");
}
