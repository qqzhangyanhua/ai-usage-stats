import { useMemo } from "react";
import { donutOption } from "../lib/chartTheme";
import type { ResolvedTheme } from "../hooks/useTheme";
import { formatCompact, formatTokens, formatUsd } from "../lib/format";
import type { CodeVolumeSummary } from "../types";
import { DonutChart } from "./DonutChart";
import { EmptyState } from "./EmptyState";
import { KpiCard, LegendRow } from "./Kpi";

export function CursorPanel({
  summary,
  theme,
}: {
  summary: CodeVolumeSummary | null;
  theme: ResolvedTheme;
}) {
  const data = summary ?? {
    commit_count: 0,
    lines_added: 0,
    composer_lines_added: 0,
    human_lines_added: 0,
    ai_percentage: null,
    total_cost: null,
    cost_unpriced: false,
    cost_per_thousand_ai_lines: null,
  };

  const option = useMemo(() => {
    const donutItems = [
      { name: "AI 生成行", value: data.composer_lines_added, color: "#8b6cff" },
      { name: "人工编写行", value: data.human_lines_added, color: "#22d3ee" },
    ];
    return donutOption(donutItems, theme);
  }, [data.composer_lines_added, data.human_lines_added, theme]);

  if (!summary) {
    return (
      <div className="panel partition">
        <EmptyState
          icon="cursor"
          title="暂无 Cursor 代码量数据"
          hint="请确认已启用 Cursor 数据源采集"
        />
      </div>
    );
  }

  return (
    <div className="stack">
      <section className="kpi-row">
        <KpiCard
          icon="sessions"
          tone="purple"
          label="提交数"
          value={formatTokens(data.commit_count)}
        />
        <KpiCard icon="trend" tone="cyan" label="新增行" value={formatTokens(data.lines_added)} />
        <KpiCard
          icon="cursor"
          tone="orange"
          label="AI 生成行"
          value={formatTokens(data.composer_lines_added)}
        />
        <KpiCard
          icon="daily"
          tone="blue"
          label="AI 占比"
          value={data.ai_percentage == null ? "—" : `${data.ai_percentage.toFixed(1)}%`}
        />
      </section>

      <div className="panel partition">
        <div className="panel-head">
          <h2>Cursor 代码量（独立口径，不计入 token）</h2>
        </div>
        <p className="note">此面板只展示 AI 生成代码行数与占比，不会并入上方任何 token 总量。</p>
        <div className="donut-wrap">
          <DonutChart option={option} centerValue={formatCompact(data.lines_added)} />
          <div className="legend-col">
            <LegendRow
              color="#8b6cff"
              label="AI 生成行"
              value={formatTokens(data.composer_lines_added)}
              extra={
                data.lines_added > 0
                  ? `${((data.composer_lines_added / data.lines_added) * 100).toFixed(1)}%`
                  : undefined
              }
            />
            <LegendRow
              color="#22d3ee"
              label="人工编写行"
              value={formatTokens(data.human_lines_added)}
              extra={
                data.lines_added > 0
                  ? `${((data.human_lines_added / data.lines_added) * 100).toFixed(1)}%`
                  : undefined
              }
            />
          </div>
        </div>
      </div>

      <CodeCostRoiCard data={data} />
    </div>
  );
}

function CodeCostRoiCard({ data }: { data: CodeVolumeSummary }) {
  const hasAiLines = data.composer_lines_added > 0;
  return (
    <div className="panel partition">
      <div className="panel-head">
        <h2>成本 × 代码量交叉指标</h2>
      </div>
      <p className="note">
        粗略 ROI 参考：分子是全部 AI CLI 来源至今的费用估算，分母只是 Cursor 记录到的 AI
        生成行数，两者统计边界不同，不做精确归因，仅供大致趋势参考。
      </p>
      <div className="roi-row">
        <div className="roi-cell">
          <span className="muted">全部来源累计费用</span>
          <strong>{formatUsd(data.total_cost, data.cost_unpriced)}</strong>
        </div>
        <div className="roi-cell">
          <span className="muted">AI 生成行（累计）</span>
          <strong>{formatTokens(data.composer_lines_added)}</strong>
        </div>
        <div className="roi-cell roi-cell-highlight">
          <span className="muted">每千行 AI 代码成本</span>
          <strong>
            {data.cost_per_thousand_ai_lines != null
              ? formatUsd(data.cost_per_thousand_ai_lines, data.cost_unpriced)
              : "—"}
          </strong>
          {!hasAiLines ? <em>暂无 AI 生成行，无法计算</em> : null}
        </div>
      </div>
    </div>
  );
}
