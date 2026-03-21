import { Navigate, Outlet, Route, Routes } from "react-router-dom";
import { useSettings } from "./hooks/useSettings";
import { AppLayout } from "./layout/AppLayout";
import { HomePage } from "./pages/HomePage";
import { PlaygroundPage } from "./pages/PlaygroundPage";
import { SessionCreatePage } from "./pages/SessionCreatePage";
import { SessionDetailPage } from "./pages/SessionDetailPage";
import { SessionsListPage } from "./pages/SessionsListPage";
import { SettingsPage } from "./pages/SettingsPage";
import { WorkersListPage } from "./pages/WorkersListPage";

function RequireStoredUrl() {
  const { controlPlaneUrl } = useSettings();
  if (!controlPlaneUrl) {
    return <Navigate to="/settings" replace />;
  }
  return <Outlet />;
}

export function App() {
  return (
    <Routes>
      <Route path="/" element={<AppLayout />}>
        <Route path="settings" element={<SettingsPage />} />
        <Route element={<RequireStoredUrl />}>
          <Route index element={<HomePage />} />
          <Route path="playground" element={<PlaygroundPage />} />
          <Route path="sessions" element={<SessionsListPage />} />
          <Route path="sessions/new" element={<SessionCreatePage />} />
          <Route path="sessions/:sessionId" element={<SessionDetailPage />} />
          <Route path="workers" element={<WorkersListPage />} />
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
