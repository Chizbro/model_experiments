import { NavLink, Outlet } from "react-router-dom";

const linkClass = ({ isActive }: { isActive: boolean }) =>
  [
    "rounded-md px-3 py-2 text-sm font-medium transition-colors",
    isActive ? "bg-primary text-primary-fg" : "text-muted hover:bg-black/5 hover:text-foreground",
  ].join(" ");

export function AppLayout() {
  return (
    <div className="flex min-h-screen flex-col">
      <header className="border-b border-border bg-card">
        <div className="mx-auto flex max-w-5xl flex-wrap items-center justify-between gap-3 px-4 py-3">
          <div className="flex items-baseline gap-3">
            <span className="text-lg font-semibold tracking-tight">Remote Harness</span>
            <span className="text-xs text-muted">control plane UI</span>
          </div>
          <nav className="flex flex-wrap gap-1">
            <NavLink to="/" className={linkClass} end>
              Home
            </NavLink>
            <NavLink to="/sessions" className={linkClass}>
              Sessions
            </NavLink>
            <NavLink to="/workers" className={linkClass}>
              Workers
            </NavLink>
            <NavLink to="/playground" className={linkClass}>
              API playground
            </NavLink>
            <NavLink to="/settings" className={linkClass}>
              Settings
            </NavLink>
          </nav>
        </div>
      </header>
      <main className="mx-auto w-full max-w-5xl flex-1 px-4 py-8">
        <Outlet />
      </main>
    </div>
  );
}
