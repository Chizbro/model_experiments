export function WorkerPoolHeterogeneityBanner({ show }: { show: boolean }) {
  if (!show) return null;
  return (
    <div
      role="alert"
      className="rounded-lg border border-amber-300 bg-amber-50 px-4 py-3 text-sm text-amber-950 dark:border-amber-700 dark:bg-amber-950/40 dark:text-amber-100"
    >
      <p className="font-medium">Heterogeneous worker pool</p>
      <p className="mt-1 text-amber-900/90 dark:text-amber-100/90">
        Two or more active workers report different platforms (or mix WSL with native Windows). The engine may assign any
        session to any worker; mixed OS or missing CLIs often produce confusing failures. Prefer a homogeneous pool per{" "}
        <span className="font-medium">Architecture §4c</span> (platform-specific CLI invocation) until label-based dispatch
        ships.
      </p>
      <p className="mt-2 text-xs text-amber-900/80 dark:text-amber-100/80">
        Full detail: <code className="rounded bg-black/10 px-1">docs/ARCHITECTURE.md</code> — §4c Platform-specific workers
        (CLI invocation).
      </p>
    </div>
  );
}
