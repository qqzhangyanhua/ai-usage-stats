import { memo, useCallback, useEffect, useMemo, useState, type KeyboardEvent } from "react";
import { areaTrendOption, formatBucket } from "../lib/chartTheme";
import { bucketToDateRange } from "../lib/calendar";
import { chartClickDataIndex } from "../lib/chartClick";
import { trendSeriesTable } from "../lib/exportRows";
import { formatCompact, formatDelta, formatUsd } from "../lib/format";
import { summarizeTrend, trendTableRowsNewestFirst } from "../lib/trendStats";
import type { ResolvedTheme } from "../hooks/useTheme";
import type { Grain, SeriesPoint } from "../types";
import { EmptyState } from "./EmptyState";
import { ExportableChart } from "./ExportableChart";
import { ExportButton } from "./ExportButton";
import { KpiCard } from "./Kpi";
import { Pagination } from "./Pagination";
import { RangeBackButton } from "./RangeBackButton";
import { GrainSwitch, grainUnit } from "./ui/GrainSwitch";

const PAGE_SIZE = 20;

export const Trend = memo(function Trend({
  grain,
  setGrain,
  points,
  theme,
  onRangeSelect,
  onRangeBack,
}: {
  grain: Grain;
  setGrain: (grain: Grain) => void;
  points: SeriesPoint[];
  theme: ResolvedTheme;
  onRangeSelect?: (from: string, to: string) => void;
  onRangeBack?: () => void;
}) {
  const [page, setPage] = useState(1);
  const option = useMemo(() => areaTrendOption(points, theme), [points, theme]);
  const stats = useMemo(() => summarizeTrend(points), [points]);
  const tableRows = useMemo(() => trendTableRowsNewestFirst(points), [points]);
  const exportTable = useMemo(() => trendSeriesTable(points), [points]);

  const pageCount = Math.max(1, Math.ceil(tableRows.length / PAGE_SIZE));
  const pagedRows = useMemo(() => {
    const start = (page - 1) * PAGE_SIZE;
    return tableRows.slice(start, start + PAGE_SIZE);
  }, [page, tableRows]);

  const rangeStart = points[0]?.bucket ?? "";
  const rangeEnd = points[points.length - 1]?.bucket ?? "";

  useEffect(() => {
    setPage(1);
  }, [grain, rangeStart, rangeEnd]);

  useEffect(() => {
    setPage((current) => Math.min(current, pageCount));
  }, [pageCount]);

  const selectBucket = useCallback(
    (bucket: string) => {
      const range = bucketToDateRange(grain, bucket);
      if (!range) {
        return;
      }
      onRangeSelect?.(range.from, range.to);
    },
    [grain, onRangeSelect],
  );

  const selectTrendPoint = useCallback(
    (params: unknown) => {
      const index = chartClickDataIndex(params);
      const point = index == null ? undefined : points[index];
      if (point) {
        selectBucket(point.bucket);
      }
    },
    [points, selectBucket],
  );

  function onRowKeyDown(event: KeyboardEvent<HTMLTableRowElement>, bucket: string) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      selectBucket(bucket);
    }
  }

  return (
    <div className="stack">
      <section className="kpi-row">
        <KpiCard
          icon="tokens"
          tone="purple"
          label="区间总 Token"
          value={formatCompact(stats.totalTokens)}
          spark={stats.sparkTokens}
        />
        <KpiCard
          icon="cost"
          tone="orange"
          label="区间总费用"
          value={formatUsd(stats.hasCost ? stats.totalCost : null, !stats.hasCost)}
          spark={stats.sparkCost}
        />
        <KpiCard
          icon="daily"
          tone="blue"
          label={`平均每${grainUnit[grain]} Token`}
          value={formatCompact(Math.round(stats.bucketAvg))}
          spark={stats.sparkTokens}
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
            <p className="panel-note">
              按{grainUnit[grain]}查看输入 / 输出 Token
              {onRangeSelect ? "。点击数据点可下钻到该时段" : ""}
              {onRangeBack ? "，返回上一级可回到之前的范围" : ""}
            </p>
          </div>
          <div className="panel-head-actions">
            {onRangeBack ? <RangeBackButton onClick={onRangeBack} /> : null}
            <GrainSwitch value={grain} onChange={setGrain} />
          </div>
        </div>
        {points.length > 0 ? (
          <ExportableChart
            option={option}
            style={{ height: 320 }}
            filename="时间趋势图"
            onEvents={onRangeSelect ? { click: selectTrendPoint } : undefined}
          />
        ) : (
          <div className="analytics-empty chart-empty">
            <EmptyState
              icon="trend"
              title="当前筛选条件下暂无趋势数据"
              hint="调整时间范围或来源后再试"
            />
          </div>
        )}
      </div>

      <div className="panel">
        <div className="panel-head">
          <h2>明细构成</h2>
          <span className="muted">共 {points.length} 个时间桶 · 最新在上</span>
          <ExportButton
            filename="时间趋势"
            headers={exportTable.headers}
            rows={exportTable.rows}
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
              {pagedRows.map((row) => {
                const { point } = row;
                const delta = formatDelta(row.periodDelta);
                return (
                  <tr
                    key={point.bucket}
                    className={onRangeSelect ? "clickable" : undefined}
                    onClick={onRangeSelect ? () => selectBucket(point.bucket) : undefined}
                    onKeyDown={onRangeSelect ? (event) => onRowKeyDown(event, point.bucket) : undefined}
                    tabIndex={onRangeSelect ? 0 : undefined}
                    title={onRangeSelect ? "点击下钻到该时段" : undefined}
                  >
                    <td>{formatBucket(point.bucket)}</td>
                    <td>
                      <span className="cell-bar" title={`占总量 ${row.shareOfTotal.toFixed(1)}%`}>
                        <i style={{ width: `${row.shareOfMax}%` }} />
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
        <Pagination
          page={page}
          pageCount={pageCount}
          totalCount={points.length}
          onPageChange={setPage}
        />
      </div>
    </div>
  );
});
