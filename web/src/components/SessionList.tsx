import { useNavigate } from 'react-router-dom';
import type { SessionSummary } from '../api/types';
import StatusBadge from './StatusBadge';

interface SessionListProps {
  sessions: SessionSummary[];
  isLoading: boolean;
}

export default function SessionList({ sessions, isLoading }: SessionListProps) {
  const navigate = useNavigate();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="h-8 w-8 animate-spin rounded-full border-4 border-blue-600 border-t-transparent" />
      </div>
    );
  }

  if (sessions.length === 0) {
    return (
      <div className="rounded-lg border-2 border-dashed border-gray-300 p-12 text-center">
        <p className="text-sm text-gray-500">No sessions yet.</p>
        <button
          onClick={() => navigate('/sessions/new')}
          className="mt-3 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700"
        >
          Create First Session
        </button>
      </div>
    );
  }

  return (
    <div className="overflow-hidden rounded-lg border bg-white shadow-sm">
      <table className="min-w-full divide-y divide-gray-200">
        <thead className="bg-gray-50">
          <tr>
            <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500">
              Session
            </th>
            <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500">
              Repository
            </th>
            <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500">
              Workflow
            </th>
            <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500">
              Status
            </th>
            <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500">
              Created
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-200">
          {sessions.map((session) => (
            <tr
              key={session.session_id}
              onClick={() => navigate(`/sessions/${session.session_id}`)}
              className="cursor-pointer hover:bg-gray-50 transition-colors"
            >
              <td className="whitespace-nowrap px-4 py-3 text-sm font-mono text-gray-900">
                {session.session_id.slice(0, 8)}...
              </td>
              <td className="whitespace-nowrap px-4 py-3 text-sm text-gray-600">
                {extractRepoName(session.repo_url)}
              </td>
              <td className="whitespace-nowrap px-4 py-3 text-sm text-gray-600">
                {formatWorkflow(session.workflow)}
              </td>
              <td className="whitespace-nowrap px-4 py-3">
                <StatusBadge status={session.status} />
              </td>
              <td className="whitespace-nowrap px-4 py-3 text-sm text-gray-500">
                {formatRelativeTime(session.created_at)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function extractRepoName(url: string): string {
  try {
    const parts = url.replace(/\.git$/, '').split('/');
    return parts.slice(-2).join('/');
  } catch {
    return url;
  }
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

function formatRelativeTime(iso: string): string {
  const now = Date.now();
  const then = new Date(iso).getTime();
  const diff = now - then;
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return 'just now';
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}
