import type { Filter } from "../../types";

export const AUTO_REFRESH_STORAGE_KEY = "ai-usage-stats:auto-refresh";

export const emptyFilter: Filter = {
  from: null,
  to: null,
  sources: [],
  models: [],
  projects: [],
  providers: [],
};

export function loadAutoRefresh(): string {
  try {
    return window.localStorage.getItem(AUTO_REFRESH_STORAGE_KEY) ?? "off";
  } catch {
    return "off";
  }
}

export type SelectedSession = { id: string; source: string };
