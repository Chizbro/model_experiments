import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { ControlPlaneHttpError, controlPlaneFetch, controlPlaneJson } from "../api/client";
import { fetchHealth } from "../api/health";
import { getIdentityCredentials, patchIdentityTokens } from "../api/identities";
import {
  readBootstrapIneligible,
  readStoredGitIdentityId,
  writeBootstrapIneligible,
  writeStoredGitIdentityId,
} from "../settings/storage";
import { useSettings } from "../hooks/useSettings";

interface PaginatedApiKeys {
  items: unknown[];
}

type OauthFlash = { variant: "success" | "error"; message: string };

function NotControlPlaneCallout() {
  return (
    <p className="rounded-md border border-amber-200 bg-amber-50/90 p-3 text-sm text-amber-950 dark:border-amber-900/50 dark:bg-amber-950/40 dark:text-amber-100">
      <span className="font-medium">This URL does not look like the Remote Harness control plane.</span>{" "}
      <code className="text-foreground">GET /health</code> should return JSON with{" "}
      <code className="text-foreground">&quot;status&quot;:&quot;ok&quot;</code>. If the browser reports CORS errors together with{" "}
      <strong>404</strong>, something else is often listening on that port (for example both{" "}
      <strong>docker compose</strong> and <strong>cargo run -p server</strong>), or the base URL points at the wrong host. Check with:{" "}
      <code className="text-foreground">curl -sS http://127.0.0.1:3000/health</code> — expect JSON and header{" "}
      <code className="text-foreground">x-remote-harness-control-plane: 1</code>. See{" "}
      <code className="text-foreground">docs/TROUBLESHOOTING.md</code> §1a.
    </p>
  );
}

export function SettingsPage() {
  const qc = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();
  const {
    controlPlaneUrl,
    setControlPlaneUrlPersisted,
    apiKey,
    setApiKeyPersisted,
    wakeUrl,
    setWakeUrlPersisted,
    suggestedControlPlaneUrl,
  } = useSettings();

  const [urlDraft, setUrlDraft] = useState(() => controlPlaneUrl ?? suggestedControlPlaneUrl);
  const [keyDraft, setKeyDraft] = useState(apiKey);
  const [wakeDraft, setWakeDraft] = useState(wakeUrl);
  const [bootstrapLabel, setBootstrapLabel] = useState("browser-bootstrap");
  const [verifyMessage, setVerifyMessage] = useState<string | null>(null);
  const [bootstrapResult, setBootstrapResult] = useState<string | null>(null);
  const [gitIdentityDraft, setGitIdentityDraft] = useState(readStoredGitIdentityId);
  const [oauthFlash, setOauthFlash] = useState<OauthFlash | null>(null);
  const [agentTokenDraft, setAgentTokenDraft] = useState("");
  const [gitPatDraft, setGitPatDraft] = useState("");
  const [byolMessage, setByolMessage] = useState<string | null>(null);

  const effectiveBase = controlPlaneUrl;
  const byolIdentityId = gitIdentityDraft.trim() || "default";
  const showBootstrap =
    Boolean(effectiveBase) && !readBootstrapIneligible(effectiveBase!) && !apiKey.trim();

  useEffect(() => {
    const success = searchParams.get("oauth_success");
    const oauthErr = searchParams.get("oauth_error");
    const oauthMsg = searchParams.get("oauth_message");
    if (!success && !oauthErr) {
      return;
    }

    if (success === "github" || success === "gitlab") {
      const label = success === "github" ? "GitHub" : "GitLab";
      setOauthFlash({
        variant: "success",
        message: `${label} sign-in completed — the control plane saved a Git token for the identity in your authorize link (identity id in this browser: “${readStoredGitIdentityId()}”).`,
      });
    } else if (oauthErr) {
      const detail = oauthMsg?.trim() ? ` ${oauthMsg.trim()}` : "";
      setOauthFlash({
        variant: "error",
        message: `Sign-in did not complete (${oauthErr}).${detail}`,
      });
    }

    const next = new URLSearchParams(searchParams);
    next.delete("oauth_success");
    next.delete("oauth_error");
    next.delete("oauth_message");
    setSearchParams(next, { replace: true });
  }, [searchParams, setSearchParams]);

  const healthQuery = useQuery({
    queryKey: ["health", effectiveBase],
    queryFn: () => fetchHealth(effectiveBase!),
    enabled: Boolean(effectiveBase),
    retry: 0,
  });

  const identityCredQuery = useQuery({
    queryKey: ["identity-credentials", effectiveBase, apiKey, byolIdentityId],
    queryFn: () => getIdentityCredentials(effectiveBase!, apiKey, byolIdentityId),
    enabled: Boolean(effectiveBase && apiKey.trim()),
    retry: 0,
  });

  const saveAgentTokenMutation = useMutation({
    mutationFn: async () => {
      if (!effectiveBase || !apiKey.trim()) {
        throw new Error("Save control plane URL and API key first.");
      }
      await patchIdentityTokens(effectiveBase, apiKey, byolIdentityId, {
        agent_token: agentTokenDraft.trim(),
      });
    },
    onSuccess: () => {
      setAgentTokenDraft("");
      setByolMessage("Agent token saved on the control plane (not shown again).");
      void qc.invalidateQueries({ queryKey: ["identity-credentials"] });
    },
    onError: (e: unknown) => {
      if (e instanceof ControlPlaneHttpError) {
        setByolMessage(`${e.mapped.title}: ${e.mapped.detail}`);
        return;
      }
      setByolMessage(e instanceof Error ? e.message : String(e));
    },
  });

  const saveGitPatMutation = useMutation({
    mutationFn: async () => {
      if (!effectiveBase || !apiKey.trim()) {
        throw new Error("Save control plane URL and API key first.");
      }
      await patchIdentityTokens(effectiveBase, apiKey, byolIdentityId, {
        git_token: gitPatDraft.trim(),
      });
    },
    onSuccess: () => {
      setGitPatDraft("");
      setByolMessage("Git token (PAT) saved. OAuth can still refresh metadata separately.");
      void qc.invalidateQueries({ queryKey: ["identity-credentials"] });
    },
    onError: (e: unknown) => {
      if (e instanceof ControlPlaneHttpError) {
        setByolMessage(`${e.mapped.title}: ${e.mapped.detail}`);
        return;
      }
      setByolMessage(e instanceof Error ? e.message : String(e));
    },
  });

  const saveUrlMutation = useMutation({
    mutationFn: async (raw: string) => {
      const trimmed = raw.trim();
      if (!trimmed) {
        throw new Error("Enter a control plane URL.");
      }
      setControlPlaneUrlPersisted(trimmed);
      await qc.invalidateQueries({ queryKey: ["health"] });
      return fetchHealth(trimmed);
    },
  });

  const saveKeyMutation = useMutation({
    mutationFn: async (key: string) => {
      setApiKeyPersisted(key);
    },
  });

  const verifyKeyMutation = useMutation({
    mutationFn: async (keyToUse: string) => {
      if (!effectiveBase) {
        throw new Error("Save a control plane URL first.");
      }
      const trimmed = keyToUse.trim();
      if (!trimmed) {
        throw new Error("Enter an API key first.");
      }
      const data = await controlPlaneJson<PaginatedApiKeys>({
        baseUrl: effectiveBase,
        path: "/api-keys?limit=5",
        method: "GET",
        apiKey: trimmed,
      });
      writeBootstrapIneligible(effectiveBase, true);
      return data;
    },
    onSuccess: () => {
      setVerifyMessage("Authenticated: GET /api-keys succeeded.");
    },
    onError: (e: unknown) => {
      if (e instanceof ControlPlaneHttpError) {
        setVerifyMessage(`${e.mapped.title}: ${e.mapped.detail}`);
        return;
      }
      setVerifyMessage(e instanceof Error ? e.message : String(e));
    },
  });

  const bootstrapMutation = useMutation({
    mutationFn: async () => {
      if (!effectiveBase) {
        throw new Error("Save a control plane URL first.");
      }
      const res = await controlPlaneFetch({
        baseUrl: effectiveBase,
        path: "/api-keys/bootstrap",
        method: "POST",
        jsonBody: { label: bootstrapLabel.trim() || undefined },
      });
      const text = await res.text();
      if (res.status === 403) {
        writeBootstrapIneligible(effectiveBase, true);
      }
      if (!res.ok) {
        throw new ControlPlaneHttpError(res.status, text);
      }
      const created = JSON.parse(text) as { key?: string };
      if (created.key) {
        setApiKeyPersisted(created.key);
        setKeyDraft(created.key);
        writeBootstrapIneligible(effectiveBase, true);
      }
      return created;
    },
    onSuccess: () => {
      setBootstrapResult("Bootstrap succeeded — your new key was saved in this browser.");
      void qc.invalidateQueries({ queryKey: ["health"] });
    },
    onError: (e: unknown) => {
      if (e instanceof ControlPlaneHttpError) {
        setBootstrapResult(`${e.mapped.title}: ${e.mapped.detail}`);
        return;
      }
      const err = e as Error & { mapped?: { title: string; detail: string } };
      if (err.mapped) {
        setBootstrapResult(`${err.mapped.title}: ${err.mapped.detail}`);
        return;
      }
      setBootstrapResult(e instanceof Error ? e.message : String(e));
    },
  });

  return (
    <div className="mx-auto max-w-xl space-y-10">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
        <p className="mt-2 text-sm text-muted">
          First-time setup: confirm the control plane URL with <code className="text-foreground">GET /health</code>, then authenticate with an API key or a one-time bootstrap when the server allows it (see{" "}
          <code className="text-foreground">docs/CLIENT_EXPERIENCE.md</code> §7). Under <strong>Identity &amp; credentials</strong>, save your <strong>agent CLI token</strong> (Cursor / Claude Code) and connect Git via OAuth or a PAT.
        </p>
      </div>

      {oauthFlash ? (
        <div
          role="status"
          className={`rounded-lg border p-4 text-sm ${
            oauthFlash.variant === "success"
              ? "border-green-200 bg-green-50/90 text-green-950 dark:border-green-900/60 dark:bg-green-950/40 dark:text-green-100"
              : "border-destructive/40 bg-destructive/10 text-destructive"
          }`}
        >
          {oauthFlash.message}
        </div>
      ) : null}

      <section className="space-y-3 rounded-lg border border-border bg-card p-5 shadow-sm">
        <h2 className="text-base font-semibold">Control plane URL</h2>
        <label className="block text-sm">
          <span className="text-muted">Base URL (no trailing slash)</span>
          <input
            className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-sm"
            value={urlDraft}
            onChange={(e) => setUrlDraft(e.target.value)}
            placeholder={suggestedControlPlaneUrl}
            autoComplete="off"
            spellCheck={false}
          />
        </label>
        <button
          type="button"
          className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-fg hover:opacity-95 disabled:opacity-50"
          disabled={saveUrlMutation.isPending}
          onClick={() => saveUrlMutation.mutate(urlDraft)}
        >
          Save &amp; verify health
        </button>
        {saveUrlMutation.isError ? (
          <p className="text-sm text-destructive">
            {(saveUrlMutation.error as Error & { mapped?: { detail: string } }).mapped?.detail ??
              (saveUrlMutation.error as Error).message}
          </p>
        ) : null}
        {saveUrlMutation.isSuccess && saveUrlMutation.data ? (
          <p className={`text-sm ${saveUrlMutation.data.ok ? "text-green-800 dark:text-green-300" : "text-destructive"}`}>
            {saveUrlMutation.data.ok ? "Health OK: " : "Health failed: "}
            {saveUrlMutation.data.snippet}
          </p>
        ) : null}
        {saveUrlMutation.isSuccess && saveUrlMutation.data && !saveUrlMutation.data.looksLikeControlPlane ? (
          <NotControlPlaneCallout />
        ) : null}
        {effectiveBase && healthQuery.data && !saveUrlMutation.isSuccess ? (
          <p className={`text-sm ${healthQuery.data.ok ? "text-muted" : "text-destructive"}`}>
            Current: {healthQuery.data.ok ? "reachable" : "error"} — {healthQuery.data.snippet}
          </p>
        ) : null}
        {effectiveBase && healthQuery.data && !healthQuery.data.looksLikeControlPlane && !saveUrlMutation.isSuccess ? (
          <NotControlPlaneCallout />
        ) : null}
      </section>

      <section className="space-y-3 rounded-lg border border-border bg-card p-5 shadow-sm">
        <h2 className="text-base font-semibold">Wake URL (optional)</h2>
        <p className="text-sm text-muted">
          If your control plane sleeps, store a GET URL that your infrastructure uses to wake it (<code className="text-foreground">docs/HOSTING.md</code> §4). The Home page offers this button when health checks fail.
        </p>
        <input
          className="w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-sm"
          value={wakeDraft}
          onChange={(e) => setWakeDraft(e.target.value)}
          placeholder="https://always-on.example/wake-harness"
          autoComplete="off"
        />
        <button
          type="button"
          className="rounded-md border border-border px-4 py-2 text-sm font-medium hover:bg-black/[0.03]"
          onClick={() => setWakeUrlPersisted(wakeDraft)}
        >
          Save wake URL
        </button>
      </section>

      <section className="space-y-3 rounded-lg border border-border bg-card p-5 shadow-sm">
        <h2 className="text-base font-semibold">API key</h2>
        <p className="text-sm text-muted">Stored in this browser only (see HOSTING threat model).</p>
        <input
          className="w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-sm"
          type="password"
          autoComplete="off"
          value={keyDraft}
          onChange={(e) => setKeyDraft(e.target.value)}
          placeholder="rh_…"
        />
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-fg hover:opacity-95"
            onClick={() => {
              setVerifyMessage(null);
              setApiKeyPersisted(keyDraft);
              verifyKeyMutation.mutate(keyDraft);
            }}
            disabled={!effectiveBase || verifyKeyMutation.isPending}
          >
            Save key &amp; verify
          </button>
          <button
            type="button"
            className="rounded-md border border-border px-4 py-2 text-sm font-medium hover:bg-black/[0.03]"
            onClick={() => {
              setVerifyMessage(null);
              saveKeyMutation.mutate(keyDraft);
            }}
          >
            Save key only
          </button>
        </div>
        {verifyMessage ? <p className="text-sm">{verifyMessage}</p> : null}
      </section>

      <section id="byol-credentials" className="space-y-4 rounded-lg border border-border bg-card p-5 shadow-sm">
        <h2 className="text-base font-semibold">Identity &amp; credentials (BYOL)</h2>
        <p className="text-sm text-muted">
          Sessions need both a <span className="font-medium text-foreground">Git</span> token and an <span className="font-medium text-foreground">agent</span> token for this identity. Git: OAuth below or optional PAT. Agent: for <span className="font-medium text-foreground">Cursor</span>, use a User API key from{" "}
          <a
            className="text-primary underline underline-offset-2"
            href="https://cursor.com/dashboard/cloud-agents"
            target="_blank"
            rel="noreferrer"
          >
            Cloud Agents
          </a>{" "}
          (not your Git PAT or Remote Harness API key). For <span className="font-medium text-foreground">Claude Code</span>, use an Anthropic API key. Paste once; the control plane stores it encrypted. CLI:{" "}
          <code className="text-foreground">cargo run -p cli -- credentials set &lt;id&gt; --agent-token …</code>.
        </p>
        <label className="block text-sm">
          <span className="text-muted">Identity id</span>
          <input
            className="mt-1 w-full max-w-md rounded-md border border-border bg-background px-3 py-2 font-mono text-sm"
            value={gitIdentityDraft}
            onChange={(e) => setGitIdentityDraft(e.target.value)}
            onBlur={() => writeStoredGitIdentityId(gitIdentityDraft)}
            placeholder="default"
            autoComplete="off"
            spellCheck={false}
          />
        </label>
        <p className="text-xs text-muted">
          Same id as <strong>New session</strong> and OAuth links. Saved in this browser when the field loses focus.
        </p>

        {apiKey.trim() && effectiveBase ? (
          <div className="rounded-md border border-border bg-background/50 px-3 py-2 text-sm">
            <span className="text-muted">Stored for </span>
            <code className="text-foreground">{byolIdentityId}</code>
            <span className="text-muted">: Git </span>
            <span className="font-medium text-foreground">
              {identityCredQuery.data?.has_git_token ? "yes" : "no"}
            </span>
            <span className="text-muted"> · Agent </span>
            <span className="font-medium text-foreground">
              {identityCredQuery.data?.has_agent_token ? "yes" : "no"}
            </span>
            <button
              type="button"
              className="ml-3 text-xs font-medium text-primary underline underline-offset-2"
              disabled={identityCredQuery.isFetching}
              onClick={() => void identityCredQuery.refetch()}
            >
              Refresh
            </button>
            {identityCredQuery.isError ? (
              <p className="mt-2 text-xs text-destructive">
                {identityCredQuery.error instanceof Error
                  ? identityCredQuery.error.message
                  : String(identityCredQuery.error)}
              </p>
            ) : null}
          </div>
        ) : (
          <p className="text-sm text-muted">Save an API key above to load credential status for this identity.</p>
        )}

        <div className="space-y-2 border-t border-border pt-4">
          <h3 className="text-sm font-semibold">Agent CLI token</h3>
          <p className="text-xs text-muted">
            Used when workers run <code className="text-foreground">cursor</code> or{" "}
            <code className="text-foreground">claude_code</code>. Never sent back from the server after save.
          </p>
          <input
            className="w-full max-w-md rounded-md border border-border bg-background px-3 py-2 font-mono text-sm"
            type="password"
            autoComplete="off"
            value={agentTokenDraft}
            onChange={(e) => {
              setByolMessage(null);
              setAgentTokenDraft(e.target.value);
            }}
            placeholder="Paste agent API key"
          />
          <button
            type="button"
            className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-fg hover:opacity-95 disabled:opacity-50"
            disabled={
              !effectiveBase || !apiKey.trim() || !agentTokenDraft.trim() || saveAgentTokenMutation.isPending
            }
            onClick={() => {
              setByolMessage(null);
              saveAgentTokenMutation.mutate();
            }}
          >
            Save agent token
          </button>
        </div>

        <div className="space-y-2 border-t border-border pt-4">
          <h3 className="text-sm font-semibold">Git sign-in (OAuth)</h3>
          <p className="text-sm text-muted">
            Opens the provider in this tab. Set <code className="text-foreground">REDIRECT_AFTER_AUTH</code> to this page (e.g.{" "}
            <code className="text-foreground">http://localhost:5173/settings#byol-credentials</code>). OAuth start does not use your API key in the request; you still need the key above for PATCH and sessions.
          </p>
        </div>

        <div className="flex flex-wrap gap-2">
          <a
            className={`inline-flex rounded-md border border-border px-4 py-2 text-sm font-medium shadow-sm hover:bg-black/[0.03] dark:hover:bg-white/[0.06] ${
              !effectiveBase ? "pointer-events-none opacity-50" : ""
            }`}
            href={
              effectiveBase
                ? `${effectiveBase}/auth/github?identity_id=${encodeURIComponent(gitIdentityDraft.trim() || "default")}`
                : "#"
            }
            onClick={(e) => {
              if (!effectiveBase) {
                e.preventDefault();
                return;
              }
              writeStoredGitIdentityId(gitIdentityDraft);
            }}
          >
            Sign in with GitHub
          </a>
          <a
            className={`inline-flex rounded-md border border-border px-4 py-2 text-sm font-medium shadow-sm hover:bg-black/[0.03] dark:hover:bg-white/[0.06] ${
              !effectiveBase ? "pointer-events-none opacity-50" : ""
            }`}
            href={
              effectiveBase
                ? `${effectiveBase}/auth/gitlab?identity_id=${encodeURIComponent(gitIdentityDraft.trim() || "default")}`
                : "#"
            }
            onClick={(e) => {
              if (!effectiveBase) {
                e.preventDefault();
                return;
              }
              writeStoredGitIdentityId(gitIdentityDraft);
            }}
          >
            Sign in with GitLab
          </a>
        </div>
        {!effectiveBase ? (
          <p className="text-sm text-destructive">Save a control plane URL above before signing in.</p>
        ) : null}

        <div className="space-y-2 border-t border-border pt-4">
          <h3 className="text-sm font-semibold">Git token (optional PAT)</h3>
          <p className="text-xs text-muted">
            Only if you do not use OAuth. Personal access token with repo scope; overwrites stored Git credentials for this identity when saved.
          </p>
          <input
            className="w-full max-w-md rounded-md border border-border bg-background px-3 py-2 font-mono text-sm"
            type="password"
            autoComplete="off"
            value={gitPatDraft}
            onChange={(e) => {
              setByolMessage(null);
              setGitPatDraft(e.target.value);
            }}
            placeholder="glpat-… or github_pat_…"
          />
          <button
            type="button"
            className="rounded-md border border-border px-4 py-2 text-sm font-medium hover:bg-black/[0.03] dark:hover:bg-white/[0.06] disabled:opacity-50"
            disabled={
              !effectiveBase || !apiKey.trim() || !gitPatDraft.trim() || saveGitPatMutation.isPending
            }
            onClick={() => {
              setByolMessage(null);
              saveGitPatMutation.mutate();
            }}
          >
            Save Git PAT
          </button>
        </div>

        {byolMessage ? <p className="text-sm text-muted">{byolMessage}</p> : null}
      </section>

      <section className="space-y-3 rounded-lg border border-border bg-card p-5 shadow-sm">
        <h2 className="text-base font-semibold">Data &amp; log retention</h2>
        <p className="text-sm text-muted">
          CLIENT_EXPERIENCE §9 / PRODUCT L5: default retention and the option to mark session or job logs{" "}
          <span className="text-foreground">retain forever</span> (on each session detail).
        </p>
        {effectiveBase && healthQuery.isSuccess && healthQuery.data?.ok ? (
          <div className="space-y-2 text-sm">
            {healthQuery.data.payload?.log_retention_days_default != null ? (
              <p>
                <span className="font-medium text-foreground">Default log retention:</span>{" "}
                {healthQuery.data.payload.log_retention_days_default} days for scheduled purge. Logs may be deleted after
                that unless the session or a job is marked retain forever.
              </p>
            ) : (
              <p className="text-muted">
                This server did not include <code className="text-foreground">log_retention_days_default</code> on{" "}
                <code className="text-foreground">GET /health</code>. Typical default is 7 days (
                <code className="text-foreground">LOG_RETENTION_DAYS_DEFAULT</code>).
              </p>
            )}
            {healthQuery.data.payload?.chat_history_max_turns != null ? (
              <p>
                <span className="font-medium text-foreground">Long chat cap:</span> up to{" "}
                {healthQuery.data.payload.chat_history_max_turns} prior user turns and the same number of assistant turns
                are included on each follow-up pull (<code className="text-foreground">CHAT_HISTORY_MAX_TURNS</code>;
                value <code className="text-foreground">0</code> disables capping).
              </p>
            ) : null}
            <p className="text-xs text-muted">
              You can delete stored log lines anytime from session detail. Older lines may still exist on worker disk if
              enabled (Architecture §6).
            </p>
          </div>
        ) : (
          <p className="text-sm text-muted">
            Save the control plane URL and wait for a successful health check to load retention values from the API.
          </p>
        )}
      </section>

      {showBootstrap ? (
        <section className="space-y-3 rounded-lg border border-amber-200 bg-amber-50/80 p-5 dark:border-amber-900/50 dark:bg-amber-950/30">
          <h2 className="text-base font-semibold">First API key (bootstrap)</h2>
          <p className="text-sm text-muted">
            Shown only when no key is stored yet. <code className="text-foreground">POST /api-keys/bootstrap</code> is disabled after the first key exists or when the server sets{" "}
            <code className="text-foreground">API_KEY</code> / <code className="text-foreground">API_KEYS</code>.
          </p>
          <label className="block text-sm">
            <span className="text-muted">Label (optional)</span>
            <input
              className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
              value={bootstrapLabel}
              onChange={(e) => setBootstrapLabel(e.target.value)}
            />
          </label>
          <button
            type="button"
            className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-fg hover:opacity-95 disabled:opacity-50"
            disabled={bootstrapMutation.isPending || !effectiveBase}
            onClick={() => {
              setBootstrapResult(null);
              bootstrapMutation.mutate();
            }}
          >
            Create first key (bootstrap)
          </button>
          {bootstrapResult ? <p className="text-sm">{bootstrapResult}</p> : null}
        </section>
      ) : null}

      <section className="rounded-lg border border-border bg-card p-5 text-sm text-muted shadow-sm">
        <p className="font-medium text-foreground">CORS</p>
        <p className="mt-2">
          If the UI origin differs from the API (including port), the control plane must list this origin in{" "}
          <code className="text-foreground">CORS_ALLOWED_ORIGINS</code>. When requests fail with <code className="text-foreground">TypeError: Failed to fetch</code> across origins, the app surfaces a CORS hint (see{" "}
          <code className="text-foreground">docs/TROUBLESHOOTING.md</code> §1a).
        </p>
      </section>
    </div>
  );
}
