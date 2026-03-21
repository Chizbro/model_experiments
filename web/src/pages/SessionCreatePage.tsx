import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { ControlPlaneHttpError } from "../api/client";
import { getIdentityCredentials } from "../api/identities";
import { createSession } from "../api/sessions";
import { listAllWorkers } from "../api/workers";
import { useSettings } from "../hooks/useSettings";
import { readStoredGitIdentityId, writeStoredGitIdentityId } from "../settings/storage";
import { shouldConfirmAgentCliAgainstPool } from "../lib/workerPoolHeterogeneity";

const WORKFLOWS = ["chat", "loop_n", "loop_until_sentinel", "inbox"] as const;
type WorkflowKind = (typeof WORKFLOWS)[number];

const AGENT_CLIS = ["claude_code", "cursor"] as const;

export function SessionCreatePage() {
  const navigate = useNavigate();
  const { controlPlaneUrl, apiKey } = useSettings();
  const base = controlPlaneUrl!;
  const key = apiKey.trim();

  const [workflow, setWorkflow] = useState<WorkflowKind>("chat");
  const [repoUrl, setRepoUrl] = useState("");
  const [gitRef, setGitRef] = useState("");
  const [identityId, setIdentityId] = useState(readStoredGitIdentityId);
  const [personaId, setPersonaId] = useState("");
  const [agentCli, setAgentCli] = useState<(typeof AGENT_CLIS)[number]>("cursor");
  const [prompt, setPrompt] = useState("");
  const [loopN, setLoopN] = useState("3");
  const [sentinel, setSentinel] = useState("");
  const [inboxAgentId, setInboxAgentId] = useState("");
  const [model, setModel] = useState("");
  const [branchMode, setBranchMode] = useState<"" | "main" | "pr">("");
  const [branchNamePrefix, setBranchNamePrefix] = useState("");
  const [retainForever, setRetainForever] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const createMut = useMutation({
    mutationFn: async () => {
      setFormError(null);
      const repo = repoUrl.trim();
      if (!repo) throw new Error("Repository URL is required.");

      const identRaw = identityId.trim();
      const identForFetch = identRaw.length === 0 || identRaw === "default" ? "default" : identRaw;
      const cred = await getIdentityCredentials(base, key, identForFetch);
      if (!cred.has_git_token || !cred.has_agent_token) {
        throw new Error(
          "CREDENTIALS: Configure both Git and agent tokens for this identity before starting a session.",
        );
      }

      const workers = await listAllWorkers(base, key);
      if (shouldConfirmAgentCliAgainstPool(workers)) {
        const ok = window.confirm(
          "No active workers or the pool is mixed-platform. Jobs may fail if a worker cannot run the chosen agent CLI. Continue anyway?",
        );
        if (!ok) throw new Error("Cancelled.");
      }

      let params: Record<string, unknown> = { agent_cli: agentCli };
      if (model.trim()) params = { ...params, model: model.trim() };
      if (branchMode) params = { ...params, branch_mode: branchMode };
      if (branchNamePrefix.trim()) params = { ...params, branch_name_prefix: branchNamePrefix.trim() };

      if (workflow === "chat") {
        if (!prompt.trim()) throw new Error("Prompt is required for chat.");
        params = { ...params, prompt: prompt.trim() };
      } else if (workflow === "loop_n") {
        if (!prompt.trim()) throw new Error("Prompt is required for loop_n.");
        const n = Number.parseInt(loopN, 10);
        if (!Number.isFinite(n) || n < 1) throw new Error("loop_n requires an integer n ≥ 1.");
        params = { ...params, prompt: prompt.trim(), n };
      } else if (workflow === "loop_until_sentinel") {
        if (!prompt.trim()) throw new Error("Prompt is required.");
        if (!sentinel.trim()) throw new Error("Sentinel substring is required for loop_until_sentinel.");
        params = { ...params, prompt: prompt.trim(), sentinel: sentinel.trim() };
      } else if (workflow === "inbox") {
        if (!inboxAgentId.trim()) throw new Error("Agent id is required for inbox workflow.");
        params = { ...params, agent_id: inboxAgentId.trim() };
      }

      const body = {
        repo_url: repo,
        workflow,
        params,
        retain_forever: retainForever || undefined,
        persona_id: personaId.trim() || undefined,
        identity_id: identRaw.length === 0 || identRaw === "default" ? undefined : identRaw,
        ref: gitRef.trim() || undefined,
      };

      return createSession(base, key, body);
    },
    onSuccess: (res) => {
      navigate(`/sessions/${res.session_id}`, { replace: true });
    },
    onError: (e: unknown) => {
      if (e instanceof Error && e.message === "Cancelled.") return;
      if (e instanceof Error && e.message.startsWith("CREDENTIALS:")) {
        setFormError(e.message);
        return;
      }
      if (e instanceof ControlPlaneHttpError) {
        const d = `${e.mapped.detail}`.toLowerCase();
        if (d.includes("token") || d.includes("credential")) {
          setFormError(`CREDENTIALS: ${e.mapped.title}: ${e.mapped.detail}`);
          return;
        }
        setFormError(`${e.mapped.title}: ${e.mapped.detail}`);
        return;
      }
      setFormError(e instanceof Error ? e.message : String(e));
    },
  });

  const showCredLink =
    formError &&
    (formError.startsWith("CREDENTIALS:") || /token|credential/i.test(formError));
  const credentialBlock = showCredLink ? (
    <p className="mt-2 text-sm">
      <Link className="text-primary underline underline-offset-2" to="/settings#byol-credentials">
        Open Settings — Identity &amp; credentials (BYOL)
      </Link>{" "}
      (agent token + Git OAuth or PAT), or{" "}
      <code className="text-foreground">cargo run -p cli -- credentials set …</code>. Both tokens are required.
    </p>
  ) : null;

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">New session</h1>
        <p className="mt-1 text-sm text-muted">
          POST /sessions — same shape as the CLI. Configure agent + Git tokens under Settings → Identity &amp; credentials.
        </p>
      </div>

      <form
        className="space-y-4 rounded-lg border border-border bg-card p-5 shadow-sm"
        onSubmit={(ev) => {
          ev.preventDefault();
          void createMut.mutate();
        }}
      >
        <label className="block text-sm" htmlFor="new-session-repo">
          <span className="text-muted">Repository URL</span>
          <input
            id="new-session-repo"
            required
            className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
            value={repoUrl}
            onChange={(e) => setRepoUrl(e.target.value)}
            placeholder="https://github.com/org/repo.git"
          />
        </label>

        <label className="block text-sm">
          <span className="text-muted">Git ref (optional, default main)</span>
          <input
            className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
            value={gitRef}
            onChange={(e) => setGitRef(e.target.value)}
            placeholder="main"
          />
        </label>

        <label className="block text-sm">
          <span className="text-muted">Workflow</span>
          <select
            className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
            value={workflow}
            onChange={(e) => setWorkflow(e.target.value as WorkflowKind)}
          >
            {WORKFLOWS.map((w) => (
              <option key={w} value={w}>
                {w}
              </option>
            ))}
          </select>
        </label>

        <label className="block text-sm">
          <span className="text-muted">Identity id</span>
          <input
            className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
            value={identityId}
            onChange={(e) => setIdentityId(e.target.value)}
            onBlur={() => writeStoredGitIdentityId(identityId)}
            placeholder="default"
          />
        </label>

        <label className="block text-sm">
          <span className="text-muted">Persona id (optional)</span>
          <input
            className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
            value={personaId}
            onChange={(e) => setPersonaId(e.target.value)}
          />
        </label>

        <label className="block text-sm">
          <span className="text-muted">Agent CLI</span>
          <select
            className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
            value={agentCli}
            onChange={(e) => setAgentCli(e.target.value as (typeof AGENT_CLIS)[number])}
          >
            {AGENT_CLIS.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        </label>

        {(workflow === "chat" || workflow === "loop_n" || workflow === "loop_until_sentinel") && (
          <label className="block text-sm" htmlFor="new-session-prompt">
            <span className="text-muted">Prompt</span>
            <textarea
              id="new-session-prompt"
              className="mt-1 min-h-[100px] w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
            />
          </label>
        )}

        {workflow === "loop_n" && (
          <label className="block text-sm">
            <span className="text-muted">Loop count (n)</span>
            <input
              type="number"
              min={1}
              className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
              value={loopN}
              onChange={(e) => setLoopN(e.target.value)}
            />
          </label>
        )}

        {workflow === "loop_until_sentinel" && (
          <label className="block text-sm">
            <span className="text-muted">Sentinel (literal substring)</span>
            <input
              className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
              value={sentinel}
              onChange={(e) => setSentinel(e.target.value)}
            />
          </label>
        )}

        {workflow === "inbox" && (
          <div className="space-y-1 text-sm">
            <label className="block">
              <span className="text-muted">Inbox agent id</span>
              <input
                className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
                value={inboxAgentId}
                onChange={(e) => setInboxAgentId(e.target.value)}
              />
            </label>
            <p className="text-xs text-muted">
              Session starts in <span className="font-medium">running</span> with no jobs. Enqueue work with{" "}
              <code className="text-xs">POST /agents/&lt;agent_id&gt;/inbox</code> (same as the detail page). The worker must
              register <code className="text-xs">POST /workers/:id/inbox-listener</code> before{" "}
              <code className="text-xs">pull</code> promotes queued rows into jobs — see{" "}
              <code className="text-xs">docs/API_OVERVIEW.md</code> §8.
            </p>
          </div>
        )}

        <label className="block text-sm">
          <span className="text-muted">Model (optional)</span>
          <input
            className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="e.g. auto (Cursor)"
          />
        </label>

        <div className="grid gap-3 sm:grid-cols-2">
          <label className="block text-sm">
            <span className="text-muted">Branch mode (optional)</span>
            <select
              className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
              value={branchMode}
              onChange={(e) => setBranchMode(e.target.value as "" | "main" | "pr")}
            >
              <option value="">(default)</option>
              <option value="main">main</option>
              <option value="pr">pr</option>
            </select>
          </label>
          <label className="block text-sm">
            <span className="text-muted">Branch name prefix (optional)</span>
            <input
              className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
              value={branchNamePrefix}
              onChange={(e) => setBranchNamePrefix(e.target.value)}
            />
          </label>
        </div>

        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={retainForever} onChange={(e) => setRetainForever(e.target.checked)} />
          <span className="text-muted">Retain logs forever</span>
        </label>

        {formError && !formError.startsWith("CREDENTIALS:") ? (
          <p className="text-sm text-destructive">{formError}</p>
        ) : null}
        {formError?.startsWith("CREDENTIALS:") ? (
          <p className="text-sm text-destructive">{formError.replace(/^CREDENTIALS:\s*/, "")}</p>
        ) : null}
        {credentialBlock}

        <div className="flex flex-wrap gap-3 pt-2">
          <button
            type="submit"
            disabled={createMut.isPending}
            className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-fg disabled:opacity-50"
          >
            {createMut.isPending ? "Starting…" : "Start session"}
          </button>
          <Link to="/sessions" className="rounded-md border border-border px-4 py-2 text-sm font-medium hover:bg-black/[0.03]">
            Cancel
          </Link>
        </div>
      </form>
    </div>
  );
}
