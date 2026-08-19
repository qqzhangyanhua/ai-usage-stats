export type SettingsTabId =
  | "general"
  | "sources"
  | "display"
  | "budget"
  | "backup"
  | "cursor"
  | "pricing";

export type SettingsTab = {
  id: SettingsTabId;
  label: string;
  anchors: readonly string[];
};
