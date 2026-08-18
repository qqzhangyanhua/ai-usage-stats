import { useMemo } from "react";
import { MODEL_OTHER_SLICE, areaTrendOption, donutOption, modelSlices } from "../lib/chartTheme";
import { bucketToDateRange } from "../lib/calendar";
import { chartClickDataIndex, chartClickName } from "../lib/chartClick";
import { formatCompact } from "../lib/format";
import type { ResolvedTheme } from "../hooks/useTheme";
import type { Grain, NamedAmount, SeriesPoint } from "../types";
import { DonutChart } from "./DonutChart";
import { ExportableChart } from "./ExportableChart";
import { LegendRow } from "./Kpi";
import { GrainSwitch } from "./ui/GrainSwitch";
import { VendorIcon } from "./VendorIcon";

export function OverviewTrend({
  trend,
  models,
  totalTokens,
  grain,
  theme,
  onGrain,
  onRangeSelect,
  onModelClick,
}: {
  trend: SeriesPoint[];
  models: NamedAmount[];
  totalTokens: number;
  grain: Grain;
  theme: ResolvedTheme;
  onGrain: (grain: Grain) => void;
  onRangeSelect?: (from: string, to: string) => void;
  onModelClick?: (model: string) => void;
}) {
  const last = trend[trend.length - 1];
  const modelItems = modelSlices(models);
  const tokenTotal = formatCompact(totalTokens);
  const trendOption = useMemo(() => areaTrendOption(trend, theme), [trend, theme]);
  const modelOption = useMemo(() => donutOption(modelItems, theme), [modelItems, theme]);

  function selectTrendPoint(params: unknown) {
    const index = chartClickDataIndex(params);
    const point = index == null ? undefined : trend[index];
    if (!point) {
      return;
    }
    const range = bucketToDateRange(grain, point.bucket);
    if (!range) {
      return;
    }
    onRangeSelect?.(range.from, range.to);
  }

  function selectModel(name: string) {
    if (name === MODEL_OTHER_SLICE) {
      return;
    }
    onModelClick?.(name);
  }

  return (
    <section className="dash-mid">
      <article className="panel trend-panel">
        <div className="panel-head">
          <h2>Token 使用趋势</h2>
          <GrainSwitch value={grain} onChange={onGrain} />
        </div>
        <div className="chart-fill">
          <ExportableChart
            option={trendOption}
            style={{ height: "100%", width: "100%" }}
            filename="总览趋势图"
            onEvents={{ click: selectTrendPoint }}
          />
        </div>
      </article>
      <div className="dash-side">
        <div className="current-strip">
          <div className="cs-main">
            <span className="cs-label">当前 Token 使用量</span>
            <strong className="cs-value">{formatCompact(last?.total_tokens ?? 0)}</strong>
          </div>
          <div className="cs-split">
            <span>
              输入 <em>{formatCompact(last?.input_tokens ?? 0)}</em>
            </span>
            <span>
              输出 <em>{formatCompact(last?.output_tokens ?? 0)}</em>
            </span>
          </div>
        </div>
        <article className="panel">
          <div className="panel-head">
            <h2>模型使用分布</h2>
          </div>
          <div className="donut-wrap">
            <DonutChart
              option={modelOption}
              centerValue={tokenTotal}
              onEvents={{
                click: (params) => {
                  const name = chartClickName(params);
                  if (name) {
                    selectModel(name);
                  }
                },
              }}
            />
            <div className="legend-col">
              {modelItems.map((item) => (
                <LegendRow
                  key={item.name}
                  color={item.color}
                  icon={<VendorIcon name={item.name} size={16} />}
                  label={item.name}
                  value={`${((item.value / Math.max(totalTokens, 1)) * 100).toFixed(1)}%`}
                  extra={formatCompact(item.value)}
                  onClick={item.name === MODEL_OTHER_SLICE ? undefined : () => selectModel(item.name)}
                />
              ))}
            </div>
          </div>
        </article>
      </div>
    </section>
  );
}
