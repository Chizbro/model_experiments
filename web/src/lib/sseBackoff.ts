const CAP_MS = 30_000;

/** Exponential backoff starting at 1s, capped at 30s (CLIENT_EXPERIENCE §4). */
export function nextBackoffMs(attemptIndex: number): number {
  const base = 1000;
  const exp = Math.min(attemptIndex, 16);
  return Math.min(base * 2 ** exp, CAP_MS);
}
