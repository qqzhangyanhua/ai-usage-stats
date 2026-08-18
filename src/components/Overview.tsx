import { memo, useMemo } from "react";
import { Icon, sourceTone } from "../icons";
import { ModelLabel, VendorIcon } from "./VendorIcon";
import { areaTrendOption, chartPalette, donutOption, modelSlices } from "../lib/chartTheme";
import type { ResolvedTheme } from "../hooks/useTheme";
import { ActivityHeatmap } from "./ActivityHeatmap";
import { BillingWindows } from "./BillingWindows";
import { OfficialQuotaPanel } from "./OfficialQuotaPanel";
import { WeeklyWindows } from "./WeeklyWindows";
import { DonutChart } from "./DonutChart";
import { ExportableChart } from "./ExportableChart";
import { EmptyState } from "./EmptyState";
import { KpiCard, LegendRow, Spark } from "./Kpi";
import { Button } from "./ui/Button";
import { GrainSwitch } from "./ui/GrainSwitch";
import {
  applicationLabel,
  deltaPct,
  formatClock,
  formatCompact,
  formatDelta,
  formatUsd,
  projectLabel,
  relativeTime,
} from "../lib/format";
import type {
  BillingWindowsDto,
  Grain,
  OfficialQuotaDto,
  NamedAmount,
  OverviewDto,
  SeriesPoint,
  SessionRow,
} from "../types";

/** 各粒度下一个 trend bucket 覆盖的分钟数，用于把「最后一个 bucket 的 token 量」换算成「每分钟速率」。 */
const BUCKET_MINUTES: Record<Grain, number> = {
  hour: 60,
  day: 24 * 60,
  week: 7 * 24 * 60,
  month: 30 * 24 * 60,
};

const emptyOverview: OverviewDto = {
  total_tokens: 0,
  input_tokens: 0,
  output_tokens: 0,
  cache_read_tokens: 0,
  cache_creation_tokens: 0,
  reasoning_tokens: 0,
  session_count: 0,
  cost: null,
  unpriced: false,
};

export const Overview = memo(function Overview({
  overview,
  billingWindows,
  officialQuota,
  previous,
  trend,
  heatmap,
  heatmapRange,
  models,
  projects,
  sessions,
  grain,
  preset,
  updatedAt,
  live,
  theme,
  onGrain,
  onOpenSessions,
}: {
  overview: OverviewDto | null;
  billingWindows: BillingWindowsDto | null;
  officialQuota: OfficialQuotaDto | null;
  previous: OverviewDto | null;
  trend: SeriesPoint[];
  heatmap: SeriesPoint[];
  heatmapRange: { from: string; to: string };
  models: NamedAmount[];
  projects: NamedAmount[];
  sessions: SessionRow[];
  grain: Grain;
  preset: string;
  updatedAt: string | null;
  live: boolean;
  theme: ResolvedTheme;
  onGrain: (grain: Grain) => void;
  onOpenSessions: () => void;
}) {
  const data = overview ?? emptyOverview;
  const palette = chartPalette(theme);
  const days = periodDays(preset, grain, trend.length);
  const dailyAvg = data.total_tokens / days;
  const last = trend[trend.length - 1];
  const rate = last ? Math.round(last.total_tokens / BUCKET_MINUTES[grain]) : 0;
  const spark = trend.map((p) => p.total_tokens);
  const recent = useMemo(
    () => [...sessions].sort((a, b) => b.ended_at.localeCompare(a.ended_at)).slice(0, 8),
    [sessions],
  );
  const topProjects = projects.slice(0, 5);
  const maxProject = topProjects[0]?.total_tokens ?? 1;
  const modelItems = modelSlices(models);
  const inputShare = data.total_tokens === 0 ? 0 : (data.input_tokens / data.total_tokens) * 100;
  const outputShare = data.total_tokens === 0 ? 0 : (data.output_tokens / data.total_tokens) * 100;
  const trendOption = useMemo(() => areaTrendOption(trend, theme), [trend, theme]);
  const modelOption = useMemo(() => donutOption(modelItems, theme), [modelItems, theme]);
  const tokenTotal = formatCompact(data.total_tokens);
  // cost 为 null 代表「本期完全未定价」，不能当 0 参与涨跌百分比计算，否则会给出误导性的涨跌提示。
  const costDelta =
    data.cost == null ? null : formatDelta(deltaPct(data.cost, previous?.cost ?? null));
  const tokenOption = useMemo(() => {
    const tokenItems = [
      { name: "输入 Token", value: data.input_tokens, color: palette.input },
      { name: "输出 Token", value: data.output_tokens, color: palette.output },
    ];
    return donutOption(tokenItems, theme);
  }, [data.input_tokens, data.output_tokens, theme, palette]);

  // 首次加载完成前 overview 为 null；不要用 emptyOverview 的全零占位渲染完整仪表盘，
  // 避免用户看到一闪而过的假 $0.00 / 0 会话数。
  if (!overview) {
    return (
      <div className="dash">
        <EmptyState icon="tokens" title="正在加载总览数据…" />
      </div>
    );
  }

  return (
    <div className="dash">
      <section className="kpi-row">
        <KpiCard
          icon="tokens"
          tone="purple"
          label="总 Token 使用量"
          value={formatCompact(data.total_tokens)}
          delta={formatDelta(deltaPct(data.total_tokens, previous?.total_tokens ?? null))}
          spark={spark}
        />
        <KpiCard
          icon="chat"
          tone="cyan"
          label="总会话数"
          value={data.session_count.toLocaleString("zh-CN")}
          delta={formatDelta(deltaPct(data.session_count, previous?.session_count ?? null))}
          spark={trend.map((p) => p.total_tokens)}
        />
        <KpiCard
          icon="cost"
          tone="orange"
          label="总费用估算"
          value={formatUsd(data.cost, data.unpriced)}
          delta={costDelta}
          spark={trend.map((p) => p.cost ?? 0)}
        />
        <KpiCard
          icon="daily"
          tone="blue"
          label="日均 Token 使用量"
          value={formatCompact(Math.round(dailyAvg))}
          delta={formatDelta(
            deltaPct(dailyAvg, previous ? previous.total_tokens / Math.max(days, 1) : null),
          )}
          spark={spark}
          live={live}
          radar
        />
      </section>

      <OfficialQuotaPanel data={officialQuota} />

      <BillingWindows data={billingWindows} />

      <WeeklyWindows
        windows={billingWindows?.weekly ?? []}
        windowDays={billingWindows?.weekly_window_days ?? 7}
      />

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
              <DonutChart option={modelOption} centerValue={tokenTotal} />
              <div className="legend-col">
                {modelItems.map((item) => (
                  <LegendRow
                    key={item.name}
                    color={item.color}
                    icon={<VendorIcon name={item.name} size={16} />}
                    label={item.name}
                    value={`${((item.value / Math.max(data.total_tokens, 1)) * 100).toFixed(1)}%`}
                    extra={formatCompact(item.value)}
                  />
                ))}
              </div>
            </div>
          </article>
        </div>
      </section>

      <ActivityHeatmap points={heatmap} range={heatmapRange} />

      <section className="dash-bottom">
        <article className="panel">
          <div className="panel-head">
            <h2>Token 使用统计</h2>
          </div>
          <div className="donut-wrap">
            <DonutChart option={tokenOption} centerValue={tokenTotal} />
            <div className="legend-col">
              <LegendRow
                color={palette.input}
                label="输入 Token"
                value={formatCompact(data.input_tokens)}
                extra={`${inputShare.toFixed(1)}%`}
              />
              <LegendRow
                color={palette.output}
                label="输出 Token"
                value={formatCompact(data.output_tokens)}
                extra={`${outputShare.toFixed(1)}%`}
              />
            </div>
          </div>
        </article>
        <article className="panel">
          <div className="panel-head">
            <h2>Top 5 项目</h2>
            <span className="muted">按 Token 使用量</span>
          </div>
          <ol className="rank-list">
            {topProjects.map((row, index) => (
              <li key={row.name}>
                <span className="rank">{index + 1}</span>
                <span className="rank-name" title={row.name}>
                  {projectLabel(row.name)}
                </span>
                <span className="rank-bar">
                  <i style={{ width: `${(row.total_tokens / maxProject) * 100}%` }} />
                </span>
                <span className="rank-val">{formatCompact(row.total_tokens)}</span>
              </li>
            ))}
            {topProjects.length === 0 ? (
              <li className="empty">
                <EmptyState compact icon="project" title="暂无项目数据" />
              </li>
            ) : null}
          </ol>
        </article>
        <article className="panel">
          <div className="panel-head">
            <h2>最近会话</h2>
            <Button variant="text" onClick={onOpenSessions}>
              查看全部
            </Button>
          </div>
          <ul className="session-list">
            {recent.map((row) => (
              <li key={`${row.source}-${row.session_id}`}>
                <span className={`src-ico ${sourceTone[row.source] ?? "tone-other"}`}>
                  {applicationLabel(row.source).slice(0, 1).toUpperCase()}
                </span>
                <div className="sess-main">
                  <div className="sess-title">{projectLabel(row.project)}</div>
                  <div className="sess-sub">
                    {row.model ? (
                      <ModelLabel name={row.model} size={14} />
                    ) : (
                      applicationLabel(row.source)
                    )}
                  </div>
                </div>
                <span className="sess-time">{relativeTime(row.ended_at)}</span>
                <span className="sess-tokens">{formatCompact(row.total_tokens)}</span>
              </li>
            ))}
            {recent.length === 0 ? (
              <li className="empty">
                <EmptyState compact icon="sessions" title="暂无会话" />
              </li>
            ) : null}
          </ul>
        </article>
      </section>

      <footer className="status-bar">
        <div className="stat-block">
          <span className="muted">费用（估算）</span>
          <strong>{formatUsd(data.cost, data.unpriced)}</strong>
          {data.unpriced ? <em>部分模型单价未配置</em> : <em>已按单价核算</em>}
        </div>
        <div className="stat-block">
          <span className="muted">缓存 / 推理</span>
          <strong>
            {formatCompact(data.cache_read_tokens + data.cache_creation_tokens)} /{" "}
            {formatCompact(data.reasoning_tokens)}
          </strong>
          <em>读+写 / 推理 Token</em>
        </div>
        <div className="stat-block">
          <span className="muted">Token 速率（估算）</span>
          <strong>
            {rate.toLocaleString("zh-CN")} <small>/min</small>
          </strong>
          <Spark values={spark} color={palette.output} />
        </div>
        <div className="stat-block last">
          <span className="muted">
            <Icon name="clock" size={13} /> 数据更新时间
          </span>
          <strong className="clock">{formatClock(updatedAt)}</strong>
        </div>
      </footer>
    </div>
  );
});

function periodDays(preset: string, grain: Grain, bucketCount: number): number {
  if (preset === "7") {
    return 7;
  }
  if (preset === "30") {
    return 30;
  }
  if (grain === "week") {
    return Math.max(bucketCount * 7, 1);
  }
  if (grain === "month") {
    return Math.max(bucketCount * 30, 1);
  }
  if (grain === "hour") {
    return Math.max(bucketCount / 24, 1);
  }
  return Math.max(bucketCount, 1);
}
