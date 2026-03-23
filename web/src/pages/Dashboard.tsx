import { useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { listSessions, ApiError } from '../api/client';
import SessionList from '../components/SessionList';

export default function Dashboard() {
  const navigate = useNavigate();

  const {
    data,
    isLoading,
    error,
  } = useQuery({
    queryKey: ['sessions'],
    queryFn: () => listSessions({ limit: 50 }),
    refetchInterval: 5_000,
    staleTime: 3_000,
  });

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Dashboard</h1>
        <button
          onClick={() => navigate('/sessions/new')}
          className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-blue-700"
        >
          New Session
        </button>
      </div>

      {error instanceof ApiError && (
        <div className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-800">
          {error.kind === 'cors'
            ? 'Browser blocked the request (CORS). The admin must add this UI origin to CORS_ALLOWED_ORIGINS.'
            : error.kind === 'unauthorized'
              ? 'Not authorized. Check Settings and verify your API key.'
              : error.kind === 'network'
                ? 'Cannot reach the control plane. Check the URL in Settings.'
                : error.message}
        </div>
      )}

      <SessionList sessions={data?.items ?? []} isLoading={isLoading} />
    </div>
  );
}
