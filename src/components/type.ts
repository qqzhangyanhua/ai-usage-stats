import type { ThemeMode } from "../hooks/useTheme";
import type { IconName } from "../icons";
import type { SettingsTabId } from "../lib/type";
import type { ConversationSessionRow } from "../types";

export type ThemeOption = {
  value: ThemeMode;
  label: string;
  icon: IconName;
  note: string;
};

export type SettingsTabIcon = Record<SettingsTabId, IconName>;

export type ConversationExportFormat = "markdown" | "json";

export type ConversationJumpBarProps = {
  atTop: boolean;
  atBottom: boolean;
  unseenCount: number;
  onJumpTop: () => void;
  onJumpBottom: () => void;
};

export type ConversationDetailHeadProps = {
  session: ConversationSessionRow;
  fileAvailable: boolean;
  breadcrumb: string | null;
  parentAvailable: boolean;
  exportFormat: ConversationExportFormat | null;
  exportStatus: string | null;
  exportError: boolean;
  exportDisabled: boolean;
  onBack: () => void;
  onExport: (format: ConversationExportFormat) => void;
};
