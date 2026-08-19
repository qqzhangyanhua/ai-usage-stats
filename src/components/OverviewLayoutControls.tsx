import type { ReactNode } from "react";
import { applicationLabel } from "../lib/format";
import {
  isModuleVisible,
  isQuotaSourceVisible,
  OVERVIEW_MODULE_IDS,
  OVERVIEW_MODULE_LABELS,
  QUOTA_SOURCE_IDS,
  setAllModulesVisible,
  setAllQuotaSourcesVisible,
  setModuleVisible,
  setQuotaSourceVisible,
  type OverviewLayout,
} from "../lib/overviewLayout";
import { Button } from "./ui/Button";

export function OverviewLayoutControls({
  layout,
  onChange,
  detectedSources = [],
}: {
  layout: OverviewLayout;
  onChange: (layout: OverviewLayout) => void;
  detectedSources?: string[];
}) {
  const detected = new Set(detectedSources);

  return (
    <div className="overview-layout-controls">
      <ToggleGroup
        title="概览模块"
        note="关掉后首页不再展示该区块，数据仍会照常采集。"
        onShowAll={() => onChange(setAllModulesVisible(layout, true))}
        onHideAll={() => onChange(setAllModulesVisible(layout, false))}
      >
        {OVERVIEW_MODULE_IDS.map((id) => (
          <ToggleChip
            key={id}
            label={OVERVIEW_MODULE_LABELS[id]}
            pressed={isModuleVisible(layout, id)}
            onToggle={() => onChange(setModuleVisible(layout, id, !isModuleVisible(layout, id)))}
          />
        ))}
      </ToggleGroup>
      <ToggleGroup
        title="额度模块中的来源"
        note="只影响 5 小时计费窗和滚动用量里的 Codex、Cursor Agent 等行，不是官方配额接口。"
        onShowAll={() => onChange(setAllQuotaSourcesVisible(layout, true))}
        onHideAll={() => onChange(setAllQuotaSourcesVisible(layout, false))}
      >
        {QUOTA_SOURCE_IDS.map((id) => (
          <ToggleChip
            key={id}
            label={applicationLabel(id)}
            pressed={isQuotaSourceVisible(layout, id)}
            badge={detected.has(id) ? "已检测" : undefined}
            onToggle={() =>
              onChange(setQuotaSourceVisible(layout, id, !isQuotaSourceVisible(layout, id)))
            }
          />
        ))}
      </ToggleGroup>
    </div>
  );
}

function ToggleGroup({
  title,
  note,
  onShowAll,
  onHideAll,
  children,
}: {
  title: string;
  note: string;
  onShowAll: () => void;
  onHideAll: () => void;
  children: ReactNode;
}) {
  return (
    <div className="overview-layout-group">
      <div className="overview-layout-group-head">
        <div>
          <h3>{title}</h3>
          <p className="panel-note">{note}</p>
        </div>
        <div className="row-actions">
          <Button size="sm" onClick={onShowAll}>
            全部显示
          </Button>
          <Button size="sm" onClick={onHideAll}>
            全部隐藏
          </Button>
        </div>
      </div>
      <div className="overview-layout-chips">{children}</div>
    </div>
  );
}

function ToggleChip({
  label,
  pressed,
  badge,
  onToggle,
}: {
  label: string;
  pressed: boolean;
  badge?: string;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      className={["layout-toggle", pressed ? "is-on" : "is-off"].join(" ")}
      aria-pressed={pressed}
      onClick={onToggle}
    >
      <span>{label}</span>
      {badge ? <em>{badge}</em> : null}
    </button>
  );
}
