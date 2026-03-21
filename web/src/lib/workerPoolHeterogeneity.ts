/** Active (non-stale) workers only — see docs/CLIENT_EXPERIENCE.md §10. */

export interface WorkerLike {
  status: string;
  labels?: Record<string, unknown>;
}

type PlatformBucket = "wsl" | "windows_native" | "other" | "unknown";

function platformBucket(labels: Record<string, unknown> | undefined): PlatformBucket {
  const raw = labels?.platform;
  if (typeof raw !== "string") return "unknown";
  const p = raw.trim().toLowerCase();
  if (!p.length) return "unknown";
  if (p === "wsl") return "wsl";
  if (p === "windows" || p === "win32" || p === "windows_nt") return "windows_native";
  return "other";
}

/**
 * Canonical key for comparing pool mix: merges native Windows spellings; keeps WSL and other OS strings distinct.
 */
export function canonicalPlatformKey(labels: Record<string, unknown> | undefined): string | undefined {
  const b = platformBucket(labels);
  if (b === "unknown") return undefined;
  if (b === "windows_native") return "__windows_native__";
  if (b === "wsl") return "wsl";
  const raw = labels?.platform;
  return typeof raw === "string" ? raw.trim().toLowerCase() : undefined;
}

export function isWorkerPoolHeterogeneous(workers: WorkerLike[]): boolean {
  const active = workers.filter((w) => w.status === "active");
  if (active.length < 2) return false;
  const keys = active.map((w) => canonicalPlatformKey(w.labels)).filter((k): k is string => Boolean(k));
  if (keys.length < 2) return false;
  return new Set(keys).size >= 2;
}

export function activeWorkers(workers: WorkerLike[]): WorkerLike[] {
  return workers.filter((w) => w.status === "active");
}

/** v1 heuristic: confirm when the pool is empty or mixed-platform per §10. */
export function shouldConfirmAgentCliAgainstPool(workers: WorkerLike[]): boolean {
  const act = activeWorkers(workers);
  if (act.length === 0) return true;
  return isWorkerPoolHeterogeneous(workers);
}
