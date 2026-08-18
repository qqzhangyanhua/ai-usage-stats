import { useMemo } from "react";
import { breakdownBarOption, codeVolumeDailyOption, donutOption } from "../lib/chartTheme";
import type { ResolvedTheme } from "../hooks/useTheme";
import { formatCompact, formatTokens, formatUsd } from "../lib/format";
import { codeVolumeTable } from "../lib/exportRows";
import type { CodeVolumeSummary } from "../types";
import { CursorCodeVolumeTable } from "./CursorCodeVolumeTable";
import { DonutChart } from "./DonutChart";
import { EmptyState } from "./EmptyState";
import { ExportButton } from "./ExportButton";
import { ExportableChart } from "./ExportableChart";
import { KpiCard, LegendRow } from "./Kpi";
import { LoadingOverlay } from "./LoadingOverlay";

function emptyVolume(): CodeVolumeSummary {
  return {
    commit_count: 0,
    lines_added: 0,
    lines_deleted: 0,
    net_lines: 0,
    composer_lines_added: 0,
    composer_lines_deleted: 0,
    human_lines_added: 0,
    human_lines_deleted: 0,
    tab_lines_added: 0,
    tab_lines_deleted: 0,
    ai_percentage: null,
    total_cost: null,
    cost_unpriced: false,
    cost_per_thousand_ai_lines: null,
    daily: [],
    by_branch: [],
    commits: [],
  };
}

export function CursorPanel({
  summary,
  loading = false,
  theme,
}: {
  summary: CodeVolumeSummary | null;
  loading?: boolean;
  theme: ResolvedTheme;
}) {
  const data = summary ?? emptyVolume();

  const option = useMemo(() => {
    const donutItems = [
      { name: "Composer", value: data.composer_lines_added, color: "#8b6cff" },
      { name: "Tab", value: data.tab_lines_added, color: "#f59e0b" },
      { name: "人工", value: data.human_lines_added, color: "#22d3ee" },
    ];
    return donutOption(donutItems, theme);
  }, [data.composer_lines_added, data.tab_lines_added, data.human_lines_added, theme]);

  const dailyOption = useMemo(
    () => codeVolumeDailyOption(data.daily, theme),
    [data.daily, theme],
  );

  const branchOption = useMemo(() => {
    const top = data.by_branch.slice(0, 8);
    const labels = top.map((row) => row.name).reverse();
    const values = top.map((row) => row.lines_added).reverse();
    return breakdownBarOption(labels, values, theme);
  }, [data.by_branch, theme]);

  if (!summary && loading) {
    return (
      <LoadingOverlay active className="panel partition">
        <EmptyState icon="cursor" title="正在加载代码量…" />
      </LoadingOverlay>
    );
  }

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
    <LoadingOverlay active={loading} className="stack">
      <section className="kpi-row">
        <KpiCard
          icon="sessions"
          tone="purple"
          label="提交数"
          value={formatTokens(data.commit_count)}
        />
        <KpiCard icon="trend" tone="cyan" label="新增行" value={formatTokens(data.lines_added)} />
        <KpiCard
          icon="filter"
          tone="orange"
          label="删除行"
          value={formatTokens(data.lines_deleted)}
        />
        <KpiCard icon="daily" tone="blue" label="净增行" value={formatTokens(data.net_lines)} />
      </section>
      <section className="kpi-row">
        <KpiCard
          icon="cursor"
          tone="orange"
          label="AI 生成行"
          value={formatTokens(data.composer_lines_added)}
        />
        <KpiCard
          icon="source"
          tone="cyan"
          label="Tab 行"
          value={formatTokens(data.tab_lines_added)}
        />
        <KpiCard
          icon="chat"
          tone="blue"
          label="人工行"
          value={formatTokens(data.human_lines_added)}
        />
        <KpiCard
          icon="daily"
          tone="purple"
          label="AI 占比"
          value={data.ai_percentage == null ? "—" : `${data.ai_percentage.toFixed(1)}%`}
        />
      </section>

      <div className="panel partition">
        <div className="panel-head">
          <h2>Cursor 代码量（独立口径，不计入 token）</h2>
          <ExportButton
            filename="Cursor代码量"
            headers={codeVolumeTable(data).headers}
            rows={codeVolumeTable(data).rows}
          />
        </div>
        <p className="note">
          Composer / Tab / 人工 是 Cursor 的归因切片，三者之和不必等于新增行。AI
          占比仍只按 Composer 新增 ÷ 新增行，Tab 不计入该百分比。
        </p>
        <div className="donut-wrap">
          <DonutChart option={option} centerValue={formatCompact(data.lines_added)} />
          <div className="legend-col">
            <LegendRow
              color="#8b6cff"
              label="Composer"
              value={formatTokens(data.composer_lines_added)}
              extra={
                data.lines_added > 0
                  ? `${((data.composer_lines_added / data.lines_added) * 100).toFixed(1)}%`
                  : undefined
              }
            />
            <LegendRow
              color="#f59e0b"
              label="Tab"
              value={formatTokens(data.tab_lines_added)}
              extra={
                data.lines_added > 0
                  ? `${((data.tab_lines_added / data.lines_added) * 100).toFixed(1)}%`
                  : undefined
              }
            />
            <LegendRow
              color="#22d3ee"
              label="人工"
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

      {data.daily.length > 0 ? (
        <section className="panel partition">
          <div className="panel-head">
            <h2>按天趋势</h2>
          </div>
          <ExportableChart
            option={dailyOption}
            filename="cursor-code-volume-daily"
            style={{ height: 280 }}
          />
        </section>
      ) : null}

      {data.by_branch.length > 0 ? (
        <section className="panel partition">
          <div className="panel-head">
            <h2>按分支</h2>
          </div>
          <p className="note">scored_commits 没有仓库路径，分支是目前唯一可切开的维度。</p>
          <ExportableChart
            option={branchOption}
            filename="cursor-code-volume-branches"
            style={{ height: Math.max(220, data.by_branch.slice(0, 8).length * 36) }}
          />
        </section>
      ) : null}

      <CursorCodeVolumeTable commits={data.commits} />
      <CodeCostRoiCard data={data} />
    </LoadingOverlay>
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
