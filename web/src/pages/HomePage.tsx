import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { fetchHealth } from "../api/health";
import { useSettings } from "../hooks/useSettings";

export function HomePage() {
  const { controlPlaneUrl, apiKey, wakeUrl } = useSettings();

  const health = useQuery({
    queryKey: ["health", controlPlaneUrl],
    queryFn: () => fetchHealth(controlPlaneUrl!),
    enabled: Boolean(controlPlaneUrl),
    retry: 1,
  });

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Home</h1>
        <p className="mt-2 max-w-2xl text-sm text-muted">
          Client-only SPA: the browser talks directly to your control plane (no UI server proxy). Configure the base URL and API key under
          Settings, then open Sessions or Workers for dashboards aligned with the CLI.
        </p>
      </div>

      <div className="rounded-lg border border-border bg-card p-5 shadow-sm">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-muted">Connection</h2>
        {!controlPlaneUrl ? (
          <p className="mt-3 text-sm">
            No control plane URL saved yet. Open{" "}
            <Link className="text-primary underline decoration-primary/30 underline-offset-2" to="/settings">
              Settings
            </Link>{" "}
            to validate <code className="text-foreground">GET /health</code> and store your URL.
          </p>
        ) : (
          <dl className="mt-3 space-y-2 text-sm">
            <div>
              <dt className="text-muted">Base URL</dt>
              <dd className="font-mono text-xs">{controlPlaneUrl}</dd>
            </div>
            <div>
              <dt className="text-muted">API key</dt>
              <dd>{apiKey.trim() ? "Saved in this browser" : "Not set"}</dd>
            </div>
            {wakeUrl.trim() ? (
              <div>
                <dt className="text-muted">Wake URL</dt>
                <dd className="break-all font-mono text-xs">{wakeUrl.trim()}</dd>
              </div>
            ) : null}
          </dl>
        )}

        {controlPlaneUrl ? (
          <div className="mt-4 border-t border-border pt-4">
            {health.isPending ? (
              <p className="text-sm text-muted">Checking health…</p>
            ) : health.isError ? (
              <div className="space-y-2">
                <p className="text-sm text-destructive">Health check failed: {(health.error as Error).message}</p>
                {wakeUrl.trim() ? (
                  <button
                    type="button"
                    className="rounded-md border border-border bg-card px-3 py-1.5 text-sm font-medium hover:bg-black/[0.03]"
                    onClick={() => {
                      void fetch(wakeUrl.trim(), { method: "GET", mode: "no-cors" }).catch(() => undefined);
                    }}
                  >
                    Wake up (GET wake URL)
                  </button>
                ) : null}
              </div>
            ) : health.data?.ok ? (
              <p className="text-sm text-green-800 dark:text-green-300">
                <span className="font-medium">Healthy.</span> <span className="text-muted">{health.data.snippet}</span>
              </p>
            ) : (
              <div className="space-y-2">
                <p className="text-sm text-destructive">
                  Health returned {health.data?.status}: {health.data?.snippet}
                </p>
                {wakeUrl.trim() ? (
                  <button
                    type="button"
                    className="rounded-md border border-border bg-card px-3 py-1.5 text-sm font-medium hover:bg-black/[0.03]"
                    onClick={() => {
                      void fetch(wakeUrl.trim(), { method: "GET", mode: "no-cors" }).catch(() => undefined);
                    }}
                  >
                    Wake up (GET wake URL)
                  </button>
                ) : null}
              </div>
            )}
          </div>
        ) : null}
      </div>

      <ul className="flex flex-wrap gap-3 text-sm">
        <li>
          <Link
            className="inline-flex rounded-md border border-border bg-card px-4 py-2 font-medium shadow-sm hover:bg-black/[0.02]"
            to="/settings"
          >
            Settings
          </Link>
        </li>
        <li>
          <Link
            className="inline-flex rounded-md border border-border bg-card px-4 py-2 font-medium shadow-sm hover:bg-black/[0.02]"
            to="/sessions"
          >
            Sessions
          </Link>
        </li>
        <li>
          <Link
            className="inline-flex rounded-md border border-border bg-card px-4 py-2 font-medium shadow-sm hover:bg-black/[0.02]"
            to="/workers"
          >
            Workers
          </Link>
        </li>
        <li>
          <Link
            className="inline-flex rounded-md border border-border bg-card px-4 py-2 font-medium shadow-sm hover:bg-black/[0.02]"
            to="/playground"
          >
            API playground
          </Link>
        </li>
      </ul>

      <p className="text-xs text-muted">
        Git OAuth: <Link to="/settings">Settings → Git sign-in</Link>. The API playground still exposes the same links for debugging.
      </p>
    </div>
  );
}
