import type { SessionStatus, JobStatus, WorkerStatus } from '../api/types';

type BadgeStatus = SessionStatus | JobStatus | WorkerStatus;

const colorMap: Record<string, string> = {
  pending: 'bg-gray-100 text-gray-700',
  assigned: 'bg-blue-50 text-blue-700',
  running: 'bg-blue-100 text-blue-700',
  completed: 'bg-green-100 text-green-700',
  failed: 'bg-red-100 text-red-700',
  active: 'bg-green-100 text-green-700',
  stale: 'bg-yellow-100 text-yellow-700',
};

interface StatusBadgeProps {
  status: BadgeStatus;
  className?: string;
}

export default function StatusBadge({ status, className = '' }: StatusBadgeProps) {
  const colors = colorMap[status] ?? 'bg-gray-100 text-gray-600';
  return (
    <span
      className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium capitalize ${colors} ${className}`}
    >
      {status}
    </span>
  );
}
