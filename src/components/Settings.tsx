import { useEffect } from "react";
import { BackupPanel } from "./BackupPanel";
import { BudgetPanel } from "./BudgetPanel";
import { CursorAccountSettingsPanel } from "./CursorAccountSettingsPanel";
import { LiteLlmSnapshotPanel } from "./LiteLlmSnapshotPanel";
import { PriceConfigPanel } from "./PriceConfigPanel";
import { PricePresetPanel } from "./PricePresetPanel";
import { SourceDiagnosticsPanel } from "./SourceDiagnosticsPanel";
import type { BudgetStatusDto, IngestReport, PriceTable, SourceDiagnostic } from "../types";

const SETTINGS_ANCHORS = [
  { id: "settings-diagnostics", label: "数据源" },
  { id: "settings-budget", label: "预算" },
  { id: "settings-backup", label: "备份" },
  { id: "settings-cursor-account", label: "Cursor 账号" },
  { id: "settings-litellm", label: "LiteLLM" },
  { id: "settings-presets", label: "预设" },
  { id: "settings-prices", label: "单价" },
] as const;

export function Settings({
  prices,
  diagnostics,
  ingestReport,
  rebuilding,
  purging,
  operationBusy,
  observedModels,
  budgetStatus,
  savingBudget,
  onChange,
  onSave,
  onRebuild,
  onPurgeArchived,
  onSnapshotRefreshed,
  onSaveBudget,
}: {
  prices: PriceTable;
  diagnostics: SourceDiagnostic[];
  ingestReport: IngestReport | null;
  rebuilding: string | null;
  purging: string | null;
  operationBusy: boolean;
  observedModels: string[];
  budgetStatus: BudgetStatusDto | null;
  savingBudget: boolean;
  onChange: (prices: PriceTable) => void;
  onSave: () => void;
  onRebuild: (source: string | null) => void;
  onPurgeArchived: (source: string | null) => void;
  onSnapshotRefreshed: () => void;
  onSaveBudget: (monthlyUsd: number | null) => void;
}) {
  useEffect(() => {
    const id = window.location.hash.replace(/^#/, "");
    if (!id.startsWith("settings-")) {
      return;
    }
    document.getElementById(id)?.scrollIntoView({ block: "start" });
  }, []);

  return (
    <div className="stack">
      <nav className="settings-toc" aria-label="设置目录">
        {SETTINGS_ANCHORS.map((anchor) => (
          <a key={anchor.id} className="filter-chip" href={`#${anchor.id}`}>
            {anchor.label}
          </a>
        ))}
      </nav>
      <SourceDiagnosticsPanel
        diagnostics={diagnostics}
        ingestReport={ingestReport}
        rebuilding={rebuilding}
        purging={purging}
        operationBusy={operationBusy}
        onRebuild={onRebuild}
        onPurgeArchived={onPurgeArchived}
      />
      <BudgetPanel status={budgetStatus} saving={savingBudget} onSave={onSaveBudget} />
      <BackupPanel onRestored={onSnapshotRefreshed} />
      <CursorAccountSettingsPanel />
      <LiteLlmSnapshotPanel onRefreshed={onSnapshotRefreshed} />
      <PricePresetPanel prices={prices} observedModels={observedModels} onChange={onChange} />
      <PriceConfigPanel prices={prices} onChange={onChange} onSave={onSave} />
    </div>
  );
}
