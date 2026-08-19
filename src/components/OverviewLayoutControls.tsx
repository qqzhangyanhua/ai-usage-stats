import { useState, type ReactNode } from "react";
import { applicationLabel } from "../lib/format";
import {
  applyDetectedQuotaSources,
  applyFavoriteQuotaSources,
  isModuleVisible,
  isQuotaSourceVisible,
  OVERVIEW_MODULE_IDS,
  OVERVIEW_MODULE_LABELS,
  quotaSourceChipIds,
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
  presentSources = [],
}: {
  layout: OverviewLayout;
  onChange: (layout: OverviewLayout) => void;
  detectedSources?: string[];
  presentSources?: string[];
}) {
  const [showAllSources, setShowAllSources] = useState(false);
  const detected = new Set(detectedSources);
  const sourceIds = quotaSourceChipIds(presentSources, showAllSources);

  return (
    <div className="overview-layout-controls">
      <ToggleGroup
        title="概览模块"
        note="关掉后首页不再展示该区块，数据仍会照常采集。"
        actions={
          <>
            <Button size="sm" onClick={() => onChange(setAllModulesVisible(layout, true))}>
              全部显示
            </Button>
            <Button size="sm" onClick={() => onChange(setAllModulesVisible(layout, false))}>
              全部隐藏
            </Button>
          </>
        }
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
        note="只影响 5 小时计费窗和滚动用量里的行，不是官方配额接口。"
        actions={
          <>
            <Button size="sm" onClick={() => onChange(setAllQuotaSourcesVisible(layout, true))}>
              全部显示
            </Button>
            <Button
              size="sm"
              disabled={detectedSources.length === 0}
              onClick={() => onChange(applyDetectedQuotaSources(layout, detectedSources))}
            >
              仅已检测
            </Button>
            <Button size="sm" onClick={() => onChange(applyFavoriteQuotaSources(layout))}>
              常用：Codex / Claude / Cursor
            </Button>
            <Button size="sm" onClick={() => onChange(setAllQuotaSourcesVisible(layout, false))}>
              全部隐藏
            </Button>
          </>
        }
      >
        {sourceIds.map((id) => (
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
      {presentSources.length > 0 ? (
        <div className="overview-layout-source-more">
          <Button size="sm" onClick={() => setShowAllSources((prev) => !prev)}>
            {showAllSources ? "只看来源有数据的项" : "显示全部来源"}
          </Button>
        </div>
      ) : null}
    </div>
  );
}

function ToggleGroup({
  title,
  note,
  actions,
  children,
}: {
  title: string;
  note: string;
  actions: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="overview-layout-group">
      <div className="overview-layout-group-head">
        <div>
          <h3>{title}</h3>
          <p className="panel-note">{note}</p>
        </div>
        <div className="row-actions">{actions}</div>
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
