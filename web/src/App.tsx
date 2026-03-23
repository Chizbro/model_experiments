import { Routes, Route, Navigate } from 'react-router-dom';
import { isConfigured } from './api/client';
import Layout from './components/Layout';
import Dashboard from './pages/Dashboard';
import SessionDetail from './pages/SessionDetail';
import SessionCreate from './pages/SessionCreate';
import Workers from './pages/Workers';
import Settings from './pages/Settings';

function RequireConfig({ children }: { children: React.ReactNode }) {
  if (!isConfigured()) {
    return <Navigate to="/settings" replace />;
  }
  return <>{children}</>;
}

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route
          path="/"
          element={
            <RequireConfig>
              <Dashboard />
            </RequireConfig>
          }
        />
        <Route
          path="/sessions/new"
          element={
            <RequireConfig>
              <SessionCreate />
            </RequireConfig>
          }
        />
        <Route
          path="/sessions/:id"
          element={
            <RequireConfig>
              <SessionDetail />
            </RequireConfig>
          }
        />
        <Route
          path="/workers"
          element={
            <RequireConfig>
              <Workers />
            </RequireConfig>
          }
        />
        <Route path="/settings" element={<Settings />} />
      </Route>
    </Routes>
  );
}
