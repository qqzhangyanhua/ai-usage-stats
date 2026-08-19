import type { Filter } from "../../types";

export const AUTO_REFRESH_STORAGE_KEY = "ai-usage-stats:auto-refresh";

export const AUTO_REFRESH_OPTIONS: { value: string; label: string }[] = [
  { value: "off", label: "关闭" },
  { value: "1", label: "每 1 分钟" },
  { value: "5", label: "每 5 分钟" },
  { value: "10", label: "每 10 分钟" },
  { value: "30", label: "每 30 分钟" },
  { value: "60", label: "每 1 小时" },
];

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
