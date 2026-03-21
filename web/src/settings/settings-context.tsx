import { createContext } from "react";

export interface SettingsContextValue {
  /** Persisted control plane base URL, or null until the user saves one in Settings. */
  controlPlaneUrl: string | null;
  setControlPlaneUrlPersisted: (url: string) => void;
  apiKey: string;
  setApiKeyPersisted: (key: string) => void;
  wakeUrl: string;
  setWakeUrlPersisted: (url: string) => void;
  /** Shown as placeholder in Settings when URL not yet stored. */
  suggestedControlPlaneUrl: string;
}

export const SettingsContext = createContext<SettingsContextValue | null>(null);
