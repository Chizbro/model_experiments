import { useCallback, useMemo, useState, type ReactNode } from "react";
import {
  envSuggestedControlPlaneUrl,
  normalizeControlPlaneUrl,
  readStoredApiKey,
  readStoredControlPlaneUrl,
  readStoredWakeUrl,
  writeStoredApiKey,
  writeStoredControlPlaneUrl,
  writeStoredWakeUrl,
} from "../settings/storage";
import { SettingsContext, type SettingsContextValue } from "../settings/settings-context";

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [controlPlaneUrl, setControlPlaneUrl] = useState<string | null>(() => readStoredControlPlaneUrl());
  const [apiKey, setApiKey] = useState(() => readStoredApiKey());
  const [wakeUrl, setWakeUrl] = useState(() => readStoredWakeUrl());

  const setControlPlaneUrlPersisted = useCallback((url: string) => {
    const n = normalizeControlPlaneUrl(url);
    writeStoredControlPlaneUrl(n);
    setControlPlaneUrl(n);
  }, []);

  const setApiKeyPersisted = useCallback((key: string) => {
    writeStoredApiKey(key);
    setApiKey(key);
  }, []);

  const setWakeUrlPersisted = useCallback((url: string) => {
    writeStoredWakeUrl(url);
    setWakeUrl(url.trim());
  }, []);

  const value = useMemo<SettingsContextValue>(
    () => ({
      controlPlaneUrl,
      setControlPlaneUrlPersisted,
      apiKey,
      setApiKeyPersisted,
      wakeUrl,
      setWakeUrlPersisted,
      suggestedControlPlaneUrl: envSuggestedControlPlaneUrl(),
    }),
    [apiKey, controlPlaneUrl, setApiKeyPersisted, setControlPlaneUrlPersisted, setWakeUrlPersisted, wakeUrl],
  );

  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}
