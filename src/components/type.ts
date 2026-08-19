import type { ThemeMode } from "../hooks/useTheme";
import type { IconName } from "../icons";
import type { SettingsTabId } from "../lib/type";

export type ThemeOption = {
  value: ThemeMode;
  label: string;
  icon: IconName;
  note: string;
};

export type SettingsTabIcon = Record<SettingsTabId, IconName>;
