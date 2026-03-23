import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery, useMutation } from '@tanstack/react-query';
import { createSession, listPersonas, ApiError } from '../api/client';
import type {
  WorkflowType,
  AgentCli,
  BranchMode,
  CreateSessionRequest,
} from '../api/types';

export default function SessionCreate() {
  const navigate = useNavigate();

  // Form state
  const [repoUrl, setRepoUrl] = useState('');
  const [workflow, setWorkflow] = useState<WorkflowType>('chat');
  const [prompt, setPrompt] = useState('');
  const [agentCli, setAgentCli] = useState<AgentCli>('claude_code');
  const [n, setN] = useState(3);
  const [sentinel, setSentinel] = useState('');
  const [ref, setRef] = useState('');
  const [branchMode, setBranchMode] = useState<BranchMode | ''>('');
  const [model, setModel] = useState('');
  const [personaId, setPersonaId] = useState('');
  const [retainForever, setRetainForever] = useState(false);

  const [error, setError] = useState<string | null>(null);

  // Load personas for the dropdown
  const { data: personasData } = useQuery({
    queryKey: ['personas'],
    queryFn: listPersonas,
    staleTime: 60_000,
  });

  const mutation = useMutation({
    mutationFn: (req: CreateSessionRequest) => createSession(req),
    onSuccess: (data) => {
      navigate(`/sessions/${data.session_id}`);
    },
    onError: (err) => {
      if (err instanceof ApiError) {
        setError(err.message);
      } else {
        setError('Failed to create session.');
      }
    },
  });

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);

    if (!repoUrl.trim()) {
      setError('Repository URL is required.');
      return;
    }
    if (!prompt.trim() && workflow !== 'inbox') {
      setError('Prompt is required.');
      return;
    }
    if (workflow === 'loop_n' && (!n || n < 1)) {
      setError('Number of iterations (n) must be at least 1.');
      return;
    }
    if (workflow === 'loop_until_sentinel' && !sentinel.trim()) {
      setError('Sentinel string is required for loop_until_sentinel workflow.');
      return;
    }

    const params: Record<string, unknown> = {
      prompt: prompt.trim(),
      agent_cli: agentCli,
    };

    if (workflow === 'loop_n') {
      params.n = n;
    }
    if (workflow === 'loop_until_sentinel') {
      params.sentinel = sentinel.trim();
    }
    if (model.trim()) {
      params.model = model.trim();
    }
    if (branchMode) {
      params.branch_mode = branchMode;
    }

    const req: CreateSessionRequest = {
      repo_url: repoUrl.trim(),
      workflow,
      params,
    };

    if (ref.trim()) {
      req.ref = ref.trim();
    }
    if (personaId) {
      req.persona_id = personaId;
    }
    if (retainForever) {
      req.retain_forever = true;
    }

    mutation.mutate(req);
  }

  return (
    <div className="mx-auto max-w-2xl">
      <h1 className="text-2xl font-bold mb-6">New Session</h1>

      {error && (
        <div className="mb-4 rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-800">
          {error}
        </div>
      )}

      <form onSubmit={handleSubmit} className="space-y-6">
        {/* Repository URL */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">
            Repository URL <span className="text-red-500">*</span>
          </label>
          <input
            type="text"
            value={repoUrl}
            onChange={(e) => setRepoUrl(e.target.value)}
            placeholder="https://github.com/owner/repo.git"
            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          />
        </div>

        {/* Workflow */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">
            Workflow <span className="text-red-500">*</span>
          </label>
          <select
            value={workflow}
            onChange={(e) => setWorkflow(e.target.value as WorkflowType)}
            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          >
            <option value="chat">Chat</option>
            <option value="loop_n">Loop N</option>
            <option value="loop_until_sentinel">Loop Until Sentinel</option>
            <option value="inbox">Inbox</option>
          </select>
        </div>

        {/* Prompt */}
        {workflow !== 'inbox' && (
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Prompt <span className="text-red-500">*</span>
            </label>
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={4}
              placeholder="Describe the task for the agent..."
              className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
            />
          </div>
        )}

        {/* Agent CLI */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">
            Agent CLI <span className="text-red-500">*</span>
          </label>
          <select
            value={agentCli}
            onChange={(e) => setAgentCli(e.target.value as AgentCli)}
            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          >
            <option value="claude_code">Claude Code</option>
            <option value="cursor">Cursor</option>
          </select>
        </div>

        {/* Conditional: N for loop_n */}
        {workflow === 'loop_n' && (
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Number of Iterations (n) <span className="text-red-500">*</span>
            </label>
            <input
              type="number"
              min={1}
              value={n}
              onChange={(e) => setN(parseInt(e.target.value, 10) || 1)}
              className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
            />
          </div>
        )}

        {/* Conditional: Sentinel for loop_until_sentinel */}
        {workflow === 'loop_until_sentinel' && (
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Sentinel String <span className="text-red-500">*</span>
            </label>
            <input
              type="text"
              value={sentinel}
              onChange={(e) => setSentinel(e.target.value)}
              placeholder="Literal substring to match in agent output"
              className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
            />
            <p className="mt-1 text-xs text-gray-500">
              Case-sensitive literal match. The loop stops when this string appears in agent output.
            </p>
          </div>
        )}

        {/* Optional fields in a collapsible section */}
        <details className="rounded-lg border bg-gray-50 p-4">
          <summary className="cursor-pointer text-sm font-medium text-gray-700">
            Advanced Options
          </summary>
          <div className="mt-4 space-y-4">
            {/* Git ref */}
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Git Ref (branch/commit)
              </label>
              <input
                type="text"
                value={ref}
                onChange={(e) => setRef(e.target.value)}
                placeholder="main"
                className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              />
            </div>

            {/* Branch mode */}
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Branch Mode
              </label>
              <select
                value={branchMode}
                onChange={(e) => setBranchMode(e.target.value as BranchMode | '')}
                className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              >
                <option value="">Default</option>
                <option value="main">Push to main</option>
                <option value="pr">Create PR/MR</option>
              </select>
            </div>

            {/* Model */}
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Model
              </label>
              <input
                type="text"
                value={model}
                onChange={(e) => setModel(e.target.value)}
                placeholder="auto (for Cursor) or default"
                className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              />
            </div>

            {/* Persona */}
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Persona
              </label>
              <select
                value={personaId}
                onChange={(e) => setPersonaId(e.target.value)}
                className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              >
                <option value="">None</option>
                {personasData?.items.map((p) => (
                  <option key={p.persona_id} value={p.persona_id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </div>

            {/* Retain forever */}
            <label className="flex items-center gap-2 text-sm text-gray-700">
              <input
                type="checkbox"
                checked={retainForever}
                onChange={(e) => setRetainForever(e.target.checked)}
                className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
              />
              Retain logs forever (exempt from automatic purge)
            </label>
          </div>
        </details>

        {/* Submit */}
        <div className="flex items-center gap-3">
          <button
            type="submit"
            disabled={mutation.isPending}
            className="rounded-md bg-blue-600 px-6 py-2.5 text-sm font-medium text-white shadow-sm hover:bg-blue-700 disabled:opacity-50"
          >
            {mutation.isPending ? 'Creating...' : 'Create Session'}
          </button>
          <button
            type="button"
            onClick={() => navigate('/')}
            className="rounded-md border border-gray-300 px-6 py-2.5 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50"
          >
            Cancel
          </button>
        </div>
      </form>
    </div>
  );
}
