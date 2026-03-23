import { useState, useEffect, useRef, useCallback } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getSession,
  deleteSession,
  updateSession,
  sendSessionInput,
  deleteSessionLogs,
  ApiError,
} from '../api/client';
import { connectSSE } from '../api/sse';
import type { SessionEvent, SessionDetail as SessionDetailType } from '../api/types';
import type { SSEConnection } from '../api/sse';
import StatusBadge from '../components/StatusBadge';
import LogViewer from '../components/LogViewer';

export default function SessionDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const [events, setEvents] = useState<SessionEvent[]>([]);
  const [chatMessage, setChatMessage] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [confirmDeleteLogs, setConfirmDeleteLogs] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const sseRef = useRef<SSEConnection | null>(null);

  const {
    data: session,
    isLoading,
    error: fetchError,
  } = useQuery({
    queryKey: ['session', id],
    queryFn: () => getSession(id!),
    enabled: !!id,
    refetchInterval: 5_000,
    staleTime: 3_000,
  });

  const isTerminal = session?.status === 'completed' || session?.status === 'failed';
  const isChat = session?.workflow === 'chat';

  // SSE session events
  useEffect(() => {
    if (!id || isTerminal) return;

    const conn = connectSSE(`/sessions/${id}/events`, {
      onEvent(eventType, data) {
        if (eventType === 'session_event') {
          try {
            const evt = JSON.parse(data) as SessionEvent;
            setEvents((prev) => [...prev, evt]);
            // Refetch session data on state change events
            queryClient.invalidateQueries({ queryKey: ['session', id] });
          } catch {
            // Ignore malformed
          }
        }
      },
      onError(err) {
        console.error('Session events SSE error:', err);
      },
    });

    sseRef.current = conn;
    return () => {
      conn.close();
      sseRef.current = null;
    };
  }, [id, isTerminal, queryClient]);

  // Cleanup
  useEffect(() => {
    return () => {
      sseRef.current?.close();
    };
  }, []);

  // Mutations
  const deleteMutation = useMutation({
    mutationFn: () => deleteSession(id!),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['sessions'] });
      navigate('/');
    },
    onError: (err) => {
      setError(err instanceof ApiError ? err.message : 'Failed to delete session.');
    },
  });

  const deleteLogsMutation = useMutation({
    mutationFn: () => deleteSessionLogs(id!),
    onSuccess: () => {
      setConfirmDeleteLogs(false);
      // Log viewer will reload on next mount
      setError(null);
    },
    onError: (err) => {
      setError(err instanceof ApiError ? err.message : 'Failed to delete logs.');
    },
  });

  const retainMutation = useMutation({
    mutationFn: (retain: boolean) => updateSession(id!, { retain_forever: retain }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['session', id] });
    },
    onError: (err) => {
      setError(err instanceof ApiError ? err.message : 'Failed to update session.');
    },
  });

  const sendMutation = useMutation({
    mutationFn: (message: string) => sendSessionInput(id!, message),
    onSuccess: () => {
      setChatMessage('');
      queryClient.invalidateQueries({ queryKey: ['session', id] });
    },
    onError: (err) => {
      setError(err instanceof ApiError ? err.message : 'Failed to send message.');
    },
  });

  const handleSendChat = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault();
      if (!chatMessage.trim()) return;
      sendMutation.mutate(chatMessage.trim());
    },
    [chatMessage, sendMutation],
  );

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="h-8 w-8 animate-spin rounded-full border-4 border-blue-600 border-t-transparent" />
      </div>
    );
  }

  if (fetchError) {
    const msg =
      fetchError instanceof ApiError
        ? fetchError.message
        : 'Failed to load session.';
    return (
      <div className="rounded-lg border border-red-200 bg-red-50 p-6 text-red-800">
        <p className="font-medium">Error</p>
        <p className="mt-1 text-sm">{msg}</p>
        <button
          onClick={() => navigate('/')}
          className="mt-3 text-sm text-blue-600 hover:underline"
        >
          Back to Dashboard
        </button>
      </div>
    );
  }

  if (!session) return null;

  return (
    <div className="flex flex-col gap-4 h-full">
      {/* Error banner */}
      {error && (
        <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-800">
          {error}
          <button
            onClick={() => setError(null)}
            className="ml-2 text-red-600 hover:underline"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* Header */}
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <div className="flex items-center gap-3">
            <h1 className="text-xl font-bold font-mono">{session.session_id.slice(0, 12)}...</h1>
            <StatusBadge status={session.status} />
          </div>
          <div className="mt-1 flex flex-wrap gap-4 text-sm text-gray-600">
            <span>Repo: {session.repo_url}</span>
            <span>Ref: {session.ref}</span>
            <span>Workflow: {formatWorkflow(session.workflow)}</span>
            <span>Created: {new Date(session.created_at).toLocaleString()}</span>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {/* Retain forever toggle */}
          <label className="flex items-center gap-2 text-sm text-gray-600 cursor-pointer">
            <input
              type="checkbox"
              checked={session.retain_forever}
              onChange={(e) => retainMutation.mutate(e.target.checked)}
              disabled={retainMutation.isPending}
              className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
            />
            Retain forever
          </label>

          {/* Delete logs button */}
          {!confirmDeleteLogs ? (
            <button
              onClick={() => setConfirmDeleteLogs(true)}
              className="rounded-md border border-gray-300 px-3 py-1.5 text-sm text-gray-600 hover:bg-gray-50"
            >
              Delete Logs
            </button>
          ) : (
            <div className="flex items-center gap-1">
              <span className="text-xs text-red-600">Confirm?</span>
              <button
                onClick={() => deleteLogsMutation.mutate()}
                disabled={deleteLogsMutation.isPending}
                className="rounded-md bg-red-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-red-700 disabled:opacity-50"
              >
                Yes, Delete
              </button>
              <button
                onClick={() => setConfirmDeleteLogs(false)}
                className="rounded-md border px-3 py-1.5 text-xs text-gray-600 hover:bg-gray-50"
              >
                Cancel
              </button>
            </div>
          )}

          {/* Delete session button */}
          {!confirmDelete ? (
            <button
              onClick={() => setConfirmDelete(true)}
              className="rounded-md border border-red-300 px-3 py-1.5 text-sm text-red-600 hover:bg-red-50"
            >
              Delete Session
            </button>
          ) : (
            <div className="flex items-center gap-1">
              <span className="text-xs text-red-600">Are you sure?</span>
              <button
                onClick={() => deleteMutation.mutate()}
                disabled={deleteMutation.isPending}
                className="rounded-md bg-red-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-red-700 disabled:opacity-50"
              >
                Yes, Delete
              </button>
              <button
                onClick={() => setConfirmDelete(false)}
                className="rounded-md border px-3 py-1.5 text-xs text-gray-600 hover:bg-gray-50"
              >
                Cancel
              </button>
            </div>
          )}
        </div>
      </div>

      {/* Jobs section */}
      <JobsList session={session} />

      {/* Session events */}
      {events.length > 0 && (
        <div className="rounded-lg border bg-white p-3">
          <h3 className="text-sm font-semibold text-gray-700 mb-2">Session Events</h3>
          <div className="space-y-1">
            {events.map((evt, i) => (
              <div key={i} className="flex items-center gap-2 text-xs text-gray-600">
                <span className="h-1.5 w-1.5 rounded-full bg-blue-500" />
                <span className="font-medium capitalize">{evt.event.replace(/_/g, ' ')}</span>
                {evt.job_id && (
                  <span className="font-mono text-gray-400">job: {evt.job_id.slice(0, 8)}</span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Log viewer */}
      <div className="relative flex-1 min-h-[300px] rounded-lg border overflow-hidden">
        <LogViewer sessionId={id!} sessionEnded={isTerminal} />
      </div>

      {/* Chat input for chat sessions */}
      {isChat && !isTerminal && (
        <form onSubmit={handleSendChat} className="flex gap-2">
          <input
            type="text"
            value={chatMessage}
            onChange={(e) => setChatMessage(e.target.value)}
            placeholder="Send a follow-up message..."
            className="flex-1 rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          />
          <button
            type="submit"
            disabled={sendMutation.isPending || !chatMessage.trim()}
            className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
          >
            {sendMutation.isPending ? 'Sending...' : 'Send'}
          </button>
        </form>
      )}
    </div>
  );
}

// ---- Jobs list sub-component ----

function JobsList({ session }: { session: SessionDetailType }) {
  if (session.jobs.length === 0) {
    return (
      <div className="rounded-lg border bg-white p-4 text-sm text-gray-500">
        No jobs yet.
      </div>
    );
  }

  return (
    <div className="overflow-hidden rounded-lg border bg-white">
      <table className="min-w-full divide-y divide-gray-200">
        <thead className="bg-gray-50">
          <tr>
            <th className="px-4 py-2 text-left text-xs font-medium uppercase text-gray-500">Job</th>
            <th className="px-4 py-2 text-left text-xs font-medium uppercase text-gray-500">Status</th>
            <th className="px-4 py-2 text-left text-xs font-medium uppercase text-gray-500">Created</th>
            <th className="px-4 py-2 text-left text-xs font-medium uppercase text-gray-500">Details</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-200">
          {session.jobs.map((job) => (
            <tr key={job.job_id} className="text-sm">
              <td className="whitespace-nowrap px-4 py-2 font-mono text-gray-700">
                {job.job_id.slice(0, 8)}
              </td>
              <td className="whitespace-nowrap px-4 py-2">
                <StatusBadge status={job.status} />
              </td>
              <td className="whitespace-nowrap px-4 py-2 text-gray-500">
                {new Date(job.created_at).toLocaleString()}
              </td>
              <td className="px-4 py-2">
                <JobDetails job={job} sessionParams={session.params} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function JobDetails({
  job,
  sessionParams,
}: {
  job: SessionDetailType['jobs'][number];
  sessionParams: SessionDetailType['params'];
}) {
  const parts: React.ReactNode[] = [];

  // Error message -- always show prominently
  if (job.error_message) {
    parts.push(
      <span key="err" className="text-red-600 font-medium">
        {job.error_message}
      </span>,
    );
  }

  // PR/MR link
  if (job.pull_request_url) {
    parts.push(
      <a
        key="pr"
        href={job.pull_request_url}
        target="_blank"
        rel="noopener noreferrer"
        className="text-blue-600 hover:underline"
      >
        PR/MR
      </a>,
    );
  } else if (
    job.status === 'completed' &&
    sessionParams.branch_mode === 'pr' &&
    !job.pull_request_url
  ) {
    // User expected a PR but none was created
    parts.push(
      <span key="no-pr" className="text-yellow-600 text-xs">
        PR/MR not created. Check logs or job status for details.
      </span>,
    );
  }

  // Commit ref
  if (job.commit_ref) {
    parts.push(
      <span key="commit" className="font-mono text-gray-500 text-xs">
        {job.commit_ref.slice(0, 8)}
      </span>,
    );
  } else if (job.status === 'completed' && !job.commit_ref) {
    parts.push(
      <span key="no-commit" className="text-gray-400 text-xs">
        No commit
      </span>,
    );
  }

  // Branch
  if (job.branch) {
    parts.push(
      <span key="branch" className="text-gray-500 text-xs">
        branch: {job.branch}
      </span>,
    );
  }

  if (parts.length === 0) return <span className="text-gray-400">--</span>;

  return <div className="flex flex-wrap items-center gap-2">{parts}</div>;
}

function formatWorkflow(wf: string): string {
  const map: Record<string, string> = {
    chat: 'Chat',
    loop_n: 'Loop N',
    loop_until_sentinel: 'Loop Until Sentinel',
    inbox: 'Inbox',
  };
  return map[wf] ?? wf;
}
