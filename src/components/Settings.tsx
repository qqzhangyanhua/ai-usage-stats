import { BackupPanel } from "./BackupPanel";
import { BudgetPanel } from "./BudgetPanel";
import { CursorAccountSettingsPanel } from "./CursorAccountSettingsPanel";
import { LiteLlmSnapshotPanel } from "./LiteLlmSnapshotPanel";
import { PriceConfigPanel } from "./PriceConfigPanel";
import { PricePresetPanel } from "./PricePresetPanel";
import { SourceDiagnosticsPanel } from "./SourceDiagnosticsPanel";
import type { BudgetStatusDto, IngestReport, PriceTable, SourceDiagnostic } from "../types";

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
  return (
    <div className="stack">
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
