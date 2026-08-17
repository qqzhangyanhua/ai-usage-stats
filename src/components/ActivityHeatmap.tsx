import { memo, useMemo } from "react";
import { calendarHeatmapOption } from "../lib/chartTheme";
import type { ResolvedTheme } from "../hooks/useTheme";
import type { SeriesPoint } from "../types";
import { ExportableChart } from "./ExportableChart";

export const ActivityHeatmap = memo(function ActivityHeatmap({
  points,
  range,
  theme,
}: {
  points: SeriesPoint[];
  range: { from: string; to: string };
  theme: ResolvedTheme;
}) {
  const option = useMemo(() => calendarHeatmapOption(points, range, theme), [points, range, theme]);

  return (
    <article className="panel heatmap-panel">
      <div className="panel-head">
        <h2>活跃热力图</h2>
        <span className="muted">近 53 周 · 按日 Token</span>
      </div>
      <div className="heatmap-chart">
        <ExportableChart
          option={option}
          style={{ height: "100%", width: "100%" }}
          filename="活跃热力图"
        />
      </div>
    </article>
  );
});
