import { useMemo } from "react";
import { donutOption } from "../lib/chartTheme";
import type { ResolvedTheme } from "../hooks/useTheme";
import { formatCompact, formatTokens } from "../lib/format";
import { codeVolumeTable } from "../lib/exportRows";
import type { CodeVolumeSummary } from "../types";
import { DonutChart } from "./DonutChart";
import { EmptyState } from "./EmptyState";
import { ExportButton } from "./ExportButton";
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
          <ExportButton
            filename="Cursor代码量"
            headers={codeVolumeTable(data).headers}
            rows={codeVolumeTable(data).rows}
          />
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
    </div>
  );
}
