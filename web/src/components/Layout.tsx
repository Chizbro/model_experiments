import { useState, useEffect, useCallback } from 'react';
import { Outlet, NavLink } from 'react-router-dom';
import { checkHealth, getControlPlaneUrl, getApiKey, ApiError } from '../api/client';

type ConnectionState = 'connected' | 'auth_failed' | 'unreachable' | 'unconfigured';

function useConnectionStatus() {
  const [state, setState] = useState<ConnectionState>('unconfigured');

  const check = useCallback(async () => {
    const url = getControlPlaneUrl();
    if (!url) {
      setState('unconfigured');
      return;
    }
    try {
      await checkHealth();
      // Health passed (no auth needed). Now verify the API key works.
      const key = getApiKey();
      if (!key) {
        setState('auth_failed');
        return;
      }
      // Try an authenticated endpoint to validate the key.
      const res = await fetch(`${url}/sessions?limit=1`, {
        headers: { Authorization: `Bearer ${key}` },
      });
      if (res.status === 401) {
        setState('auth_failed');
      } else {
        setState('connected');
      }
    } catch (err) {
      if (err instanceof ApiError && err.kind === 'cors') {
        setState('unreachable');
      } else {
        setState('unreachable');
      }
    }
  }, []);

  useEffect(() => {
    check();
    const interval = setInterval(check, 30_000);
    return () => clearInterval(interval);
  }, [check]);

  return { state, recheck: check };
}

const statusConfig: Record<ConnectionState, { color: string; label: string }> = {
  connected: { color: 'bg-green-500', label: 'Connected' },
  auth_failed: { color: 'bg-yellow-500', label: 'Auth Failed' },
  unreachable: { color: 'bg-red-500', label: 'Unreachable' },
  unconfigured: { color: 'bg-gray-400', label: 'Not Configured' },
};

const navItems = [
  { to: '/', label: 'Dashboard', icon: DashboardIcon },
  { to: '/workers', label: 'Workers', icon: WorkersIcon },
  { to: '/settings', label: 'Settings', icon: SettingsIcon },
];

export default function Layout() {
  const { state } = useConnectionStatus();
  const { color, label } = statusConfig[state];
  const [sidebarOpen, setSidebarOpen] = useState(false);

  return (
    <div className="flex h-screen overflow-hidden">
      {/* Mobile overlay */}
      {sidebarOpen && (
        <div
          className="fixed inset-0 z-30 bg-black/50 lg:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

      {/* Sidebar */}
      <aside
        className={`
          fixed inset-y-0 left-0 z-40 w-64 transform bg-gray-900 text-white transition-transform duration-200
          lg:static lg:translate-x-0
          ${sidebarOpen ? 'translate-x-0' : '-translate-x-full'}
        `}
      >
        <div className="flex h-16 items-center gap-2 border-b border-gray-700 px-6">
          <span className="text-lg font-bold tracking-tight">Remote Harness</span>
        </div>

        <nav className="mt-4 flex flex-col gap-1 px-3">
          {navItems.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.to === '/'}
              onClick={() => setSidebarOpen(false)}
              className={({ isActive }) =>
                `flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                  isActive
                    ? 'bg-gray-800 text-white'
                    : 'text-gray-400 hover:bg-gray-800 hover:text-white'
                }`
              }
            >
              <item.icon />
              {item.label}
            </NavLink>
          ))}
        </nav>

        {/* Connection status at bottom of sidebar */}
        <div className="absolute bottom-0 left-0 right-0 border-t border-gray-700 p-4">
          <div className="flex items-center gap-2 text-sm text-gray-400">
            <span className={`inline-block h-2.5 w-2.5 rounded-full ${color}`} />
            {label}
          </div>
        </div>
      </aside>

      {/* Main content */}
      <div className="flex flex-1 flex-col overflow-hidden">
        {/* Top bar (mobile) */}
        <header className="flex h-16 items-center gap-4 border-b bg-white px-4 lg:hidden">
          <button
            onClick={() => setSidebarOpen(true)}
            className="rounded-md p-1.5 text-gray-600 hover:bg-gray-100"
            aria-label="Open sidebar"
          >
            <svg className="h-6 w-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
            </svg>
          </button>
          <span className="font-semibold">Remote Harness</span>
          <div className="ml-auto flex items-center gap-2 text-sm text-gray-500">
            <span className={`inline-block h-2 w-2 rounded-full ${color}`} />
            {label}
          </div>
        </header>

        <main className="flex-1 overflow-y-auto bg-gray-50 p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}

// ---- Icons (inline SVG) ----

function DashboardIcon() {
  return (
    <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-4 0a1 1 0 01-1-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 01-1 1" />
    </svg>
  );
}

function WorkersIcon() {
  return (
    <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
    </svg>
  );
}

function SettingsIcon() {
  return (
    <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
    </svg>
  );
}
