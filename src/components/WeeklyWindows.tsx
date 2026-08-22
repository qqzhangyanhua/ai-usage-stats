import { memo } from "react";
import { formatCompact, formatUsd, weeklyCountLabel } from "../lib/format";
import type { WeeklyWindowDto } from "../types";
import { EmptyState } from "./EmptyState";
import { SourceIcon } from "./SourceIcon";

export const WeeklyWindows = memo(function WeeklyWindows({
  windows,
  windowDays,
}: {
  windows: WeeklyWindowDto[];
  windowDays: number;
}) {
  const maxTokens = Math.max(1, ...windows.map((w) => w.total_tokens));

  if (windows.length === 0) {
    return <EmptyState compact icon="clock" title={`最近 ${windowDays} 天没有用量记录`} />;
  }

  return (
    <ul className="weekly-list">
      {windows.map((window) => (
        <WeeklyRow key={window.source} window={window} maxTokens={maxTokens} />
      ))}
    </ul>
  );
});

function WeeklyRow({ window, maxTokens }: { window: WeeklyWindowDto; maxTokens: number }) {
  const progress = Math.min(100, (window.total_tokens / maxTokens) * 100);
  const dailyAvgTokens = formatCompact(Math.round(window.daily_average_tokens));
  const dailyAvgCost =
    window.daily_average_cost != null
      ? formatUsd(window.daily_average_cost, window.unpriced)
      : null;

  return (
    <li>
      <SourceIcon source={window.source} size={16} />
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
          日均 {dailyAvgTokens} Token{dailyAvgCost ? ` · 日均 ${dailyAvgCost}` : ""} ·{" "}
          {weeklyCountLabel(window.source, window.session_count)}
        </div>
      </div>
    </li>
  );
}
