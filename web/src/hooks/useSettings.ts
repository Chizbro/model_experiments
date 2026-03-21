import { useContext } from "react";
import { SettingsContext, type SettingsContextValue } from "../settings/settings-context";

export function useSettings(): SettingsContextValue {
  const v = useContext(SettingsContext);
  if (!v) {
    throw new Error("useSettings must be used within SettingsProvider");
  }
  return v;
}
