import { useState, useEffect, useCallback } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  getControlPlaneUrl,
  setControlPlaneUrl,
  getApiKey,
  setApiKey,
  checkHealth,
  getIdentity,
  getAuthStatus,
  updateIdentity,
  bootstrapApiKey,
  createApiKey,
  listApiKeys,
  revokeApiKey,
  ApiError,
} from '../api/client';
import type { ApiKeyInfo, IdentityStatus, AuthStatus } from '../api/types';

type ToastType = 'success' | 'error';

interface Toast {
  type: ToastType;
  message: string;
}

export default function Settings() {
  const queryClient = useQueryClient();

  // Connection settings
  const [url, setUrl] = useState(getControlPlaneUrl());
  const [key, setKey] = useState(getApiKey());
  const [showKey, setShowKey] = useState(false);
  const [urlValidating, setUrlValidating] = useState(false);
  const [keyValidating, setKeyValidating] = useState(false);

  // Credentials
  const [identityStatus, setIdentityStatus] = useState<IdentityStatus | null>(null);
  const [authStatus, setAuthStatus] = useState<AuthStatus | null>(null);
  const [gitToken, setGitToken] = useState('');
  const [agentToken, setAgentToken] = useState('');
  const [savingCredentials, setSavingCredentials] = useState(false);

  // Bootstrap
  const [showBootstrap, setShowBootstrap] = useState(false);
  const [bootstrapping, setBootstrapping] = useState(false);
  const [bootstrapKey, setBootstrapKey] = useState('');

  // Toast
  const [toast, setToast] = useState<Toast | null>(null);

  const showToast = useCallback((type: ToastType, message: string) => {
    setToast({ type, message });
    setTimeout(() => setToast(null), 5000);
  }, []);

  // Load credential status when connection is configured
  const loadCredentialStatus = useCallback(async () => {
    try {
      const [identity, auth] = await Promise.all([
        getIdentity('default'),
        getAuthStatus('default').catch(() => null),
      ]);
      setIdentityStatus(identity);
      setAuthStatus(auth);
    } catch {
      // Not connected or error
    }
  }, []);

  useEffect(() => {
    if (getControlPlaneUrl() && getApiKey()) {
      loadCredentialStatus();
    }
  }, [loadCredentialStatus]);

  // Check for OAuth callback parameters
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.get('credentials') === 'github_ok' || params.get('credentials') === 'gitlab_ok') {
      showToast('success', 'Git credentials saved successfully.');
      loadCredentialStatus();
      // Clean up the URL
      window.history.replaceState({}, '', '/settings');
    }
  }, [showToast, loadCredentialStatus]);

  async function handleSaveUrl() {
    if (!url.trim()) {
      showToast('error', 'Please enter a control plane URL.');
      return;
    }
    setUrlValidating(true);
    try {
      // Temporarily set the URL so checkHealth uses it
      setControlPlaneUrl(url.trim());
      await checkHealth();
      showToast('success', 'Control plane is reachable.');
    } catch (err) {
      // Revert on failure
      const msg =
        err instanceof ApiError
          ? err.message
          : 'Cannot reach the control plane. Check the URL.';
      showToast('error', msg);
      // Don't revert -- user can try again
    } finally {
      setUrlValidating(false);
    }
  }

  async function handleSaveKey() {
    if (!key.trim()) {
      showToast('error', 'Please enter an API key.');
      return;
    }
    setKeyValidating(true);
    try {
      setApiKey(key.trim());
      const cpUrl = getControlPlaneUrl();
      if (!cpUrl) {
        showToast('error', 'Set the control plane URL first.');
        setKeyValidating(false);
        return;
      }
      // Validate with an authenticated request
      const res = await fetch(`${cpUrl}/sessions?limit=1`, {
        headers: { Authorization: `Bearer ${key.trim()}` },
      });
      if (res.status === 401) {
        showToast('error', 'API key is not valid. The server returned 401.');
        return;
      }
      showToast('success', 'API key saved and validated.');
      queryClient.invalidateQueries();
      loadCredentialStatus();
    } catch (err) {
      const msg =
        err instanceof ApiError
          ? err.message
          : 'Could not validate the API key.';
      showToast('error', msg);
    } finally {
      setKeyValidating(false);
    }
  }

  async function handleBootstrap() {
    setBootstrapping(true);
    try {
      const resp = await bootstrapApiKey('web-ui-bootstrap');
      setBootstrapKey(resp.key);
      setKey(resp.key);
      setApiKey(resp.key);
      showToast('success', 'Bootstrap key created. It is shown below -- save it now.');
      setShowBootstrap(false);
      loadCredentialStatus();
    } catch (err) {
      const msg =
        err instanceof ApiError
          ? err.message
          : 'Bootstrap failed. A key may already exist.';
      showToast('error', msg);
    } finally {
      setBootstrapping(false);
    }
  }

  async function handleSaveCredentials() {
    setSavingCredentials(true);
    try {
      const body: { agent_token?: string; git_token?: string } = {};
      if (agentToken.trim()) body.agent_token = agentToken.trim();
      if (gitToken.trim()) body.git_token = gitToken.trim();
      if (!body.agent_token && !body.git_token) {
        showToast('error', 'Enter at least one token to save.');
        setSavingCredentials(false);
        return;
      }
      await updateIdentity('default', body);
      showToast('success', 'Credentials updated.');
      setAgentToken('');
      setGitToken('');
      loadCredentialStatus();
    } catch (err) {
      const msg =
        err instanceof ApiError ? err.message : 'Failed to save credentials.';
      showToast('error', msg);
    } finally {
      setSavingCredentials(false);
    }
  }

  // Detect if bootstrap should be offered
  useEffect(() => {
    async function probeBootstrap() {
      const cpUrl = getControlPlaneUrl();
      if (!cpUrl) return;
      try {
        await checkHealth();
        // If health works but we have no key, try to see if bootstrap is available
        if (!getApiKey()) {
          setShowBootstrap(true);
        }
      } catch {
        // can't reach
      }
    }
    probeBootstrap();
  }, [url]);

  const oauthBaseUrl = getControlPlaneUrl();

  return (
    <div className="mx-auto max-w-2xl space-y-8">
      <h1 className="text-2xl font-bold">Settings</h1>

      {/* Toast */}
      {toast && (
        <div
          className={`rounded-lg px-4 py-3 text-sm font-medium ${
            toast.type === 'success'
              ? 'bg-green-50 text-green-800 border border-green-200'
              : 'bg-red-50 text-red-800 border border-red-200'
          }`}
        >
          {toast.message}
        </div>
      )}

      {/* Control Plane URL */}
      <section className="rounded-lg border bg-white p-6 shadow-sm">
        <h2 className="text-lg font-semibold mb-4">Control Plane Connection</h2>
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Control Plane URL
            </label>
            <div className="flex gap-2">
              <input
                type="url"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://harness.example.com"
                className="flex-1 rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              />
              <button
                onClick={handleSaveUrl}
                disabled={urlValidating}
                className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
              >
                {urlValidating ? 'Checking...' : 'Save & Verify'}
              </button>
            </div>
            <p className="mt-1 text-xs text-gray-500">
              Validated with GET /health (no auth required).
            </p>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              API Key
            </label>
            <div className="flex gap-2">
              <div className="relative flex-1">
                <input
                  type={showKey ? 'text' : 'password'}
                  value={key}
                  onChange={(e) => setKey(e.target.value)}
                  placeholder="Enter your API key"
                  className="w-full rounded-md border border-gray-300 px-3 py-2 pr-16 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                />
                <button
                  type="button"
                  onClick={() => setShowKey(!showKey)}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-xs text-gray-500 hover:text-gray-700"
                >
                  {showKey ? 'Hide' : 'Show'}
                </button>
              </div>
              <button
                onClick={handleSaveKey}
                disabled={keyValidating}
                className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
              >
                {keyValidating ? 'Validating...' : 'Save & Verify'}
              </button>
            </div>
          </div>

          {/* Bootstrap key */}
          {showBootstrap && !getApiKey() && (
            <div className="rounded-lg border border-yellow-200 bg-yellow-50 p-4">
              <p className="text-sm text-yellow-800 mb-2">
                No API key configured. If this is a fresh server with no keys, you can bootstrap the first key.
              </p>
              <button
                onClick={handleBootstrap}
                disabled={bootstrapping}
                className="rounded-md bg-yellow-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-yellow-700 disabled:opacity-50"
              >
                {bootstrapping ? 'Creating...' : 'Bootstrap First API Key'}
              </button>
            </div>
          )}

          {/* Show bootstrap key */}
          {bootstrapKey && (
            <div className="rounded-lg border border-green-200 bg-green-50 p-4">
              <p className="text-sm font-medium text-green-800 mb-1">
                Bootstrap key created. Copy and store it now -- it will not be shown again.
              </p>
              <code className="block break-all rounded bg-white p-2 text-xs text-green-900 border">
                {bootstrapKey}
              </code>
            </div>
          )}
        </div>
      </section>

      {/* Credentials (BYOL) */}
      <section className="rounded-lg border bg-white p-6 shadow-sm">
        <h2 className="text-lg font-semibold mb-4">Credentials (Default Identity)</h2>

        {/* Current status */}
        {identityStatus && (
          <div className="mb-4 flex gap-4">
            <CredentialPill
              label="Git Token"
              configured={identityStatus.has_git_token}
            />
            <CredentialPill
              label="Agent Token"
              configured={identityStatus.has_agent_token}
            />
          </div>
        )}

        {/* Auth status */}
        {authStatus && authStatus.git_token_status !== 'not_configured' && (
          <div className="mb-4 rounded-md border p-3 text-sm">
            <span className="font-medium">Git token status: </span>
            <TokenStatusBadge status={authStatus.git_token_status} />
            {authStatus.message && (
              <span className="ml-2 text-gray-600">{authStatus.message}</span>
            )}
            {(authStatus.git_token_status === 'expired_needs_reauth' ||
              authStatus.git_token_status === 'expiring_soon') && (
              <span className="ml-2 text-orange-600 font-medium">
                Re-authenticate below.
              </span>
            )}
          </div>
        )}

        {/* OAuth buttons */}
        {oauthBaseUrl && (
          <div className="mb-4 flex gap-3">
            <a
              href={`${oauthBaseUrl}/auth/github?identity_id=default`}
              className="inline-flex items-center gap-2 rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50"
            >
              <GithubIcon />
              Sign in with GitHub
            </a>
            <a
              href={`${oauthBaseUrl}/auth/gitlab?identity_id=default`}
              className="inline-flex items-center gap-2 rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50"
            >
              <GitlabIcon />
              Sign in with GitLab
            </a>
          </div>
        )}

        {/* Manual token entry */}
        <div className="space-y-3">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Git Token (GitHub PAT / GitLab token)
            </label>
            <input
              type="password"
              value={gitToken}
              onChange={(e) => setGitToken(e.target.value)}
              placeholder="ghp_... or glpat-..."
              className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Agent Token (Claude Code / Cursor API key)
            </label>
            <input
              type="password"
              value={agentToken}
              onChange={(e) => setAgentToken(e.target.value)}
              placeholder="Enter agent API key"
              className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
            />
          </div>
          <button
            onClick={handleSaveCredentials}
            disabled={savingCredentials}
            className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
          >
            {savingCredentials ? 'Saving...' : 'Save Credentials'}
          </button>
        </div>
      </section>

      {/* API Keys Management */}
      <ApiKeysSection showToast={showToast} />

      {/* Info */}
      <section className="rounded-lg border bg-white p-6 shadow-sm">
        <h2 className="text-lg font-semibold mb-2">About</h2>
        <p className="text-sm text-gray-600">
          The API key is stored in browser localStorage. Treat this browser as a high-trust
          surface. Use HTTPS in production and do not run untrusted third-party scripts.
        </p>
        <p className="mt-2 text-sm text-gray-600">
          Logs older than the default retention period (7 days) may be deleted unless marked
          "retain forever" on a session or job.
        </p>
      </section>
    </div>
  );
}

// ---- API Keys Section ----

function ApiKeysSection({ showToast }: { showToast: (type: ToastType, message: string) => void }) {
  const [newKeyLabel, setNewKeyLabel] = useState('');
  const [createdKey, setCreatedKey] = useState('');
  const [creating, setCreating] = useState(false);
  const queryClient = useQueryClient();

  const { data: keysData, isLoading: keysLoading } = useQuery({
    queryKey: ['apiKeys'],
    queryFn: listApiKeys,
    staleTime: 10_000,
  });

  async function handleCreateKey() {
    setCreating(true);
    try {
      const resp = await createApiKey(newKeyLabel.trim() || undefined);
      setCreatedKey(resp.key);
      setNewKeyLabel('');
      queryClient.invalidateQueries({ queryKey: ['apiKeys'] });
      showToast('success', 'API key created. Copy it now -- it will not be shown again.');
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : 'Failed to create API key.';
      showToast('error', msg);
    } finally {
      setCreating(false);
    }
  }

  async function handleRevoke(keyId: string) {
    try {
      await revokeApiKey(keyId);
      queryClient.invalidateQueries({ queryKey: ['apiKeys'] });
      showToast('success', 'API key revoked.');
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : 'Failed to revoke API key.';
      showToast('error', msg);
    }
  }

  return (
    <section className="rounded-lg border bg-white p-6 shadow-sm">
      <h2 className="text-lg font-semibold mb-4">API Keys</h2>

      {/* Create new key */}
      <div className="mb-4">
        <div className="flex gap-2">
          <input
            type="text"
            value={newKeyLabel}
            onChange={(e) => setNewKeyLabel(e.target.value)}
            placeholder="Key label (optional)"
            className="flex-1 rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          />
          <button
            onClick={handleCreateKey}
            disabled={creating}
            className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
          >
            {creating ? 'Creating...' : 'Create Key'}
          </button>
        </div>
      </div>

      {/* Show newly created key */}
      {createdKey && (
        <div className="mb-4 rounded-lg border border-green-200 bg-green-50 p-4">
          <p className="text-sm font-medium text-green-800 mb-1">
            New API key created. Copy and store it now -- it will not be shown again.
          </p>
          <code className="block break-all rounded bg-white p-2 text-xs text-green-900 border">
            {createdKey}
          </code>
          <button
            onClick={() => setCreatedKey('')}
            className="mt-2 text-xs text-green-700 hover:underline"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* List existing keys */}
      {keysLoading ? (
        <p className="text-sm text-gray-500">Loading keys...</p>
      ) : keysData?.items && keysData.items.length > 0 ? (
        <div className="overflow-hidden rounded-lg border">
          <table className="min-w-full divide-y divide-gray-200">
            <thead className="bg-gray-50">
              <tr>
                <th className="px-4 py-2 text-left text-xs font-medium uppercase text-gray-500">ID</th>
                <th className="px-4 py-2 text-left text-xs font-medium uppercase text-gray-500">Label</th>
                <th className="px-4 py-2 text-left text-xs font-medium uppercase text-gray-500">Created</th>
                <th className="px-4 py-2 text-right text-xs font-medium uppercase text-gray-500">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-200 bg-white">
              {keysData.items.map((key: ApiKeyInfo) => (
                <tr key={key.id} className="text-sm">
                  <td className="whitespace-nowrap px-4 py-2 font-mono text-gray-700">
                    {key.id.slice(0, 12)}...
                  </td>
                  <td className="px-4 py-2 text-gray-600">{key.label ?? '-'}</td>
                  <td className="whitespace-nowrap px-4 py-2 text-gray-500">
                    {new Date(key.created_at).toLocaleDateString()}
                  </td>
                  <td className="px-4 py-2 text-right">
                    <button
                      onClick={() => handleRevoke(key.id)}
                      className="text-xs text-red-600 hover:underline"
                    >
                      Revoke
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="text-sm text-gray-500">No API keys found. Create one above or use bootstrap.</p>
      )}
    </section>
  );
}

// ---- Helper components ----

function CredentialPill({ label, configured }: { label: string; configured: boolean }) {
  return (
    <div
      className={`inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium ${
        configured
          ? 'bg-green-100 text-green-700'
          : 'bg-red-100 text-red-700'
      }`}
    >
      <span
        className={`h-1.5 w-1.5 rounded-full ${
          configured ? 'bg-green-500' : 'bg-red-500'
        }`}
      />
      {label}: {configured ? 'Set' : 'Missing'}
    </div>
  );
}

function TokenStatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    healthy: 'text-green-700 bg-green-50',
    expiring_soon: 'text-yellow-700 bg-yellow-50',
    expired_refreshable: 'text-orange-700 bg-orange-50',
    expired_needs_reauth: 'text-red-700 bg-red-50',
    unknown: 'text-gray-700 bg-gray-50',
    not_configured: 'text-gray-500 bg-gray-50',
  };
  return (
    <span className={`rounded px-1.5 py-0.5 text-xs font-medium ${colors[status] ?? colors.unknown}`}>
      {status.replace(/_/g, ' ')}
    </span>
  );
}

function GithubIcon() {
  return (
    <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z" />
    </svg>
  );
}

function GitlabIcon() {
  return (
    <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
      <path d="M23.955 13.587l-1.342-4.135-2.664-8.189a.455.455 0 00-.867 0L16.418 9.45H7.582L4.918 1.263a.455.455 0 00-.867 0L1.387 9.452.045 13.587a.924.924 0 00.331 1.023L12 23.054l11.624-8.443a.92.92 0 00.331-1.024" />
    </svg>
  );
}
