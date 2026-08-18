import { memo } from "react";
import { sourceTone } from "../icons";
import { applicationLabel, formatCompact, formatUsd } from "../lib/format";
import type { WeeklyWindowDto } from "../types";
import { EmptyState } from "./EmptyState";

export const WeeklyWindows = memo(function WeeklyWindows({
  windows,
  windowDays,
}: {
  windows: WeeklyWindowDto[];
  windowDays: number;
}) {
  const maxTokens = Math.max(1, ...windows.map((w) => w.total_tokens));

  return (
    <article className="panel weekly-panel">
      <div className="panel-head">
        <h2>{windowDays} 天滚动用量</h2>
        <span className="muted">按来源统计最近 {windowDays} 天的累计消耗，非官方配额</span>
      </div>
      {windows.length === 0 ? (
        <EmptyState compact icon="clock" title={`最近 ${windowDays} 天没有用量记录`} />
      ) : (
        <ul className="weekly-list">
          {windows.map((window) => (
            <WeeklyRow key={window.source} window={window} maxTokens={maxTokens} />
          ))}
        </ul>
      )}
    </article>
  );
});

function WeeklyRow({ window, maxTokens }: { window: WeeklyWindowDto; maxTokens: number }) {
  const progress = Math.min(100, (window.total_tokens / maxTokens) * 100);
  const dailyAvgTokens = formatCompact(Math.round(window.daily_average_tokens));
  const dailyAvgCost =
    window.daily_average_cost != null ? formatUsd(window.daily_average_cost, window.unpriced) : null;

  return (
    <li>
      <span className={`src-ico ${sourceTone[window.source] ?? "tone-other"}`}>
        {applicationLabel(window.source).slice(0, 1)}
      </span>
      <div className="weekly-list-main">
        <div className="weekly-list-head">
          <strong>{window.application}</strong>
          <span className="weekly-list-tokens">{formatCompact(window.total_tokens)}</span>
          <span className="weekly-list-cost">{formatUsd(window.cost, window.unpriced)}</span>
        </div>
        <div className="weekly-bar" aria-hidden="true">
          <i style={{ width: `${progress}%` }} />
        </div>
        <div className="weekly-list-meta muted">
          日均 {dailyAvgTokens} Token{dailyAvgCost ? ` · 日均 ${dailyAvgCost}` : ""} · 共{" "}
          {window.session_count} 个会话
        </div>
      </div>
    </li>
  );
}
