import { useState, useMemo } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { listWorkers, deleteWorker, ApiError } from '../api/client';
import type { Worker } from '../api/types';
import StatusBadge from '../components/StatusBadge';

export default function Workers() {
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState<string | null>(null);

  const {
    data,
    isLoading,
    error: fetchError,
  } = useQuery({
    queryKey: ['workers'],
    queryFn: listWorkers,
    refetchInterval: 10_000,
    staleTime: 5_000,
  });

  const workers = useMemo(() => data?.items ?? [], [data?.items]);

  // Heterogeneity detection
  const heterogeneityWarning = useMemo(() => {
    const nonStaleWorkers = workers.filter((w) => w.status !== 'stale');
    if (nonStaleWorkers.length < 2) return null;

    const platforms = new Set<string>();
    for (const w of nonStaleWorkers) {
      const platform = w.labels?.platform;
      if (platform) platforms.add(platform);
    }

    if (platforms.size <= 1) return null;

    const platformList = Array.from(platforms).join(', ');
    const hasWslMix =
      platforms.has('wsl') && platforms.has('windows');

    return {
      platformList,
      hasWslMix,
    };
  }, [workers]);

  const deleteMutation = useMutation({
    mutationFn: (workerId: string) => deleteWorker(workerId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['workers'] });
      setConfirmingDelete(null);
      setError(null);
    },
    onError: (err) => {
      setError(err instanceof ApiError ? err.message : 'Failed to delete worker.');
      setConfirmingDelete(null);
    },
  });

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-bold">Workers</h1>

      {/* Error banner */}
      {(error || fetchError) && (
        <div className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-800">
          {error ??
            (fetchError instanceof ApiError
              ? fetchError.message
              : 'Failed to load workers.')}
        </div>
      )}

      {/* Heterogeneity warning */}
      {heterogeneityWarning && (
        <div className="rounded-lg border border-yellow-200 bg-yellow-50 p-4">
          <div className="flex items-start gap-2">
            <WarningIcon />
            <div className="text-sm text-yellow-800">
              <p className="font-semibold">Mixed worker platforms detected</p>
              <p className="mt-1">
                Active workers have different platform labels:{' '}
                <span className="font-mono font-medium">{heterogeneityWarning.platformList}</span>.
                The engine may assign any session to any worker. Mixed OS environments or missing
                CLIs can cause confusing failures.
              </p>
              {heterogeneityWarning.hasWslMix && (
                <p className="mt-1">
                  WSL and native Windows workers are present. CLI invocation differs between these
                  environments.
                </p>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Workers table */}
      {isLoading ? (
        <div className="flex items-center justify-center py-12">
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-blue-600 border-t-transparent" />
        </div>
      ) : workers.length === 0 ? (
        <div className="rounded-lg border-2 border-dashed border-gray-300 p-12 text-center">
          <p className="text-sm text-gray-500">No workers registered.</p>
          <p className="mt-1 text-xs text-gray-400">
            Start a worker binary and point it at this control plane to register.
          </p>
        </div>
      ) : (
        <WorkersTable
          workers={workers}
          confirmingDelete={confirmingDelete}
          setConfirmingDelete={setConfirmingDelete}
          onDelete={(id) => deleteMutation.mutate(id)}
          deleting={deleteMutation.isPending}
        />
      )}
    </div>
  );
}

function WorkersTable({
  workers,
  confirmingDelete,
  setConfirmingDelete,
  onDelete,
  deleting,
}: {
  workers: Worker[];
  confirmingDelete: string | null;
  setConfirmingDelete: (id: string | null) => void;
  onDelete: (id: string) => void;
  deleting: boolean;
}) {
  return (
    <div className="overflow-hidden rounded-lg border bg-white shadow-sm">
      <table className="min-w-full divide-y divide-gray-200">
        <thead className="bg-gray-50">
          <tr>
            <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500">
              Worker ID
            </th>
            <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500">
              Host
            </th>
            <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500">
              Platform
            </th>
            <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500">
              Status
            </th>
            <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500">
              Last Seen
            </th>
            <th className="px-4 py-3 text-right text-xs font-medium uppercase tracking-wider text-gray-500">
              Actions
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-200">
          {workers.map((worker) => (
            <tr key={worker.worker_id} className="text-sm">
              <td className="whitespace-nowrap px-4 py-3 font-mono text-gray-900">
                {worker.worker_id.length > 16
                  ? worker.worker_id.slice(0, 16) + '...'
                  : worker.worker_id}
              </td>
              <td className="whitespace-nowrap px-4 py-3 text-gray-600">
                {worker.host}
              </td>
              <td className="whitespace-nowrap px-4 py-3 text-gray-600">
                {worker.labels?.platform ?? '(unknown)'}
              </td>
              <td className="whitespace-nowrap px-4 py-3">
                <StatusBadge status={worker.status} />
              </td>
              <td className="whitespace-nowrap px-4 py-3 text-gray-500">
                {formatRelativeTime(worker.last_seen_at)}
              </td>
              <td className="whitespace-nowrap px-4 py-3 text-right">
                {confirmingDelete === worker.worker_id ? (
                  <div className="flex items-center justify-end gap-1">
                    <span className="text-xs text-red-600">Confirm?</span>
                    <button
                      onClick={() => onDelete(worker.worker_id)}
                      disabled={deleting}
                      className="rounded bg-red-600 px-2 py-1 text-xs font-medium text-white hover:bg-red-700 disabled:opacity-50"
                    >
                      Yes
                    </button>
                    <button
                      onClick={() => setConfirmingDelete(null)}
                      className="rounded border px-2 py-1 text-xs text-gray-600 hover:bg-gray-50"
                    >
                      No
                    </button>
                  </div>
                ) : (
                  <button
                    onClick={() => setConfirmingDelete(worker.worker_id)}
                    className="rounded border border-red-200 px-2 py-1 text-xs text-red-600 hover:bg-red-50"
                  >
                    Delete
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
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

function WarningIcon() {
  return (
    <svg className="h-5 w-5 shrink-0 text-yellow-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4.5c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z"
      />
    </svg>
  );
}
