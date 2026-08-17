import { memo, useMemo } from "react";
import { barTrendOption, formatBucket } from "../lib/chartTheme";
import type { ResolvedTheme } from "../hooks/useTheme";
import { deltaPct, formatCompact, formatDelta, formatUsd } from "../lib/format";
import type { Grain, SeriesPoint } from "../types";
import { EmptyState } from "./EmptyState";
import { ExportableChart } from "./ExportableChart";
import { ExportButton } from "./ExportButton";
import { KpiCard } from "./Kpi";
import { GrainSwitch, grainUnit } from "./ui/GrainSwitch";

export const Trend = memo(function Trend({
  grain,
  setGrain,
  points,
  theme,
}: {
  grain: Grain;
  setGrain: (grain: Grain) => void;
  points: SeriesPoint[];
  theme: ResolvedTheme;
}) {
  const option = useMemo(() => barTrendOption(points, theme), [points, theme]);

  const stats = useMemo(() => {
    const totalTokens = points.reduce((sum, p) => sum + p.total_tokens, 0);
    const hasCost = points.some((p) => p.cost != null);
    const totalCost = points.reduce((sum, p) => sum + (p.cost ?? 0), 0);
    const dailyAvg = points.length > 0 ? totalTokens / points.length : 0;
    const peak = points.reduce<SeriesPoint | null>(
      (best, point) => (!best || point.total_tokens > best.total_tokens ? point : best),
      null,
    );
    return { totalTokens, hasCost, totalCost, dailyAvg, peak };
  }, [points]);

  const sparkTokens = points.map((p) => p.total_tokens);
  const sparkCost = points.map((p) => p.cost ?? 0);
  const maxTotal = Math.max(1, ...points.map((p) => p.total_tokens));

  return (
    <div className="stack">
      <section className="kpi-row">
        <KpiCard
          icon="tokens"
          tone="purple"
          label="区间总 Token"
          value={formatCompact(stats.totalTokens)}
          spark={sparkTokens}
        />
        <KpiCard
          icon="cost"
          tone="orange"
          label="区间总费用"
          value={formatUsd(stats.hasCost ? stats.totalCost : null, !stats.hasCost)}
          spark={sparkCost}
        />
        <KpiCard
          icon="daily"
          tone="blue"
          label={`平均每${grainUnit[grain]} Token`}
          value={formatCompact(Math.round(stats.dailyAvg))}
          spark={sparkTokens}
        />
        <KpiCard
          icon="trend"
          tone="cyan"
          label="峰值时段"
          value={stats.peak ? formatCompact(stats.peak.total_tokens) : "—"}
          delta={stats.peak ? { text: formatBucket(stats.peak.bucket), tone: "flat" } : null}
        />
      </section>

      <div className="panel">
        <div className="panel-head">
          <div>
            <h2>时间趋势</h2>
            <p className="panel-note">按{grainUnit[grain]}汇总当前筛选范围内的 Token 消耗</p>
          </div>
          <GrainSwitch value={grain} onChange={setGrain} />
        </div>
        <ExportableChart option={option} style={{ height: 320 }} filename="时间趋势图" />
      </div>

      <div className="panel">
        <div className="panel-head">
          <h2>明细构成</h2>
          <span className="muted">共 {points.length} 个时间桶</span>
          <ExportButton
            filename="时间趋势"
            headers={["时间", "总量", "输入", "输出", "费用"]}
            rows={points.map((point) => [
              formatBucket(point.bucket),
              point.total_tokens,
              point.input_tokens,
              point.output_tokens,
              point.cost ?? "",
            ])}
          />
        </div>
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>时间</th>
                <th>占比</th>
                <th>总量</th>
                <th>输入</th>
                <th>输出</th>
                <th>费用</th>
                <th>环比</th>
              </tr>
            </thead>
            <tbody>
              {points.map((point, index) => {
                const prev = points[index - 1];
                const delta = prev
                  ? formatDelta(deltaPct(point.total_tokens, prev.total_tokens))
                  : null;
                return (
                  <tr key={point.bucket}>
                    <td>{formatBucket(point.bucket)}</td>
                    <td>
                      <span className="cell-bar">
                        <i style={{ width: `${(point.total_tokens / maxTotal) * 100}%` }} />
                      </span>
                    </td>
                    <td>{formatCompact(point.total_tokens)}</td>
                    <td>{formatCompact(point.input_tokens)}</td>
                    <td>{formatCompact(point.output_tokens)}</td>
                    <td>{formatUsd(point.cost, point.cost == null)}</td>
                    <td>
                      {delta ? (
                        <span className={`delta ${delta.tone}`}>
                          {delta.text.replace(" vs 上期", "")}
                        </span>
                      ) : (
                        "—"
                      )}
                    </td>
                  </tr>
                );
              })}
              {points.length === 0 ? (
                <tr>
                  <td colSpan={7} className="analytics-empty">
                    <EmptyState icon="trend" title="暂无趋势数据" />
                  </td>
                </tr>
              ) : null}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
});
