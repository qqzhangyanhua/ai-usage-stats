import { useMemo } from "react";
import { breakdownBarOption, cursorSessionDailyOption } from "../lib/chartTheme";
import type { ResolvedTheme } from "../hooks/useTheme";
import { formatTokens, projectLabel } from "../lib/format";
import type { CursorSessionSummaryDto } from "../types";
import { EmptyState } from "./EmptyState";
import { ExportableChart } from "./ExportableChart";
import { KpiCard } from "./Kpi";

export function CursorSessionPanel({
  summary,
  theme,
}: {
  summary: CursorSessionSummaryDto | null;
  theme: ResolvedTheme;
}) {
  const data = summary ?? {
    as_of: null,
    session_count: 0,
    turn_count: 0,
    error_rate: null,
    active_project_count: 0,
    by_project: [],
    daily: [],
  };

  const trendOption = useMemo(
    () => cursorSessionDailyOption(data.daily, theme),
    [data.daily, theme],
  );

  const projectOption = useMemo(() => {
    const top = data.by_project.slice(0, 8);
    const labels = top.map((row) => projectLabel(row.name)).reverse();
    const values = top.map((row) => row.session_count).reverse();
    return breakdownBarOption(labels, values, theme);
  }, [data.by_project, theme]);

  if (!summary || summary.session_count === 0) {
    return (
      <div className="panel partition">
        <EmptyState
          icon="cursor"
          title="暂无 Cursor 会话数据"
          hint="请确认本机已有 Cursor Agent 对话，并已启用自动刷新或手动刷新。"
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
          label="会话数"
          value={formatTokens(data.session_count)}
        />
        <KpiCard icon="trend" tone="cyan" label="轮次数" value={formatTokens(data.turn_count)} />
        <KpiCard
          icon="filter"
          tone="orange"
          label="失败率"
          value={data.error_rate == null ? "—" : `${(data.error_rate * 100).toFixed(1)}%`}
        />
        <KpiCard
          icon="project"
          tone="blue"
          label="活跃项目"
          value={formatTokens(data.active_project_count)}
        />
      </section>

      <section className="panel partition">
        <div className="panel-head">
          <h2>按天趋势</h2>
        </div>
        <p className="note">独立口径，不计入 token 总量；按会话最后活跃日分桶。</p>
        <ExportableChart
          option={trendOption}
          filename="cursor-session-daily"
          style={{ height: 280 }}
        />
      </section>

      <section className="panel partition">
        <div className="panel-head">
          <h2>按项目</h2>
        </div>
        {data.by_project.length > 0 ? (
          <>
            <ExportableChart
              option={projectOption}
              filename="cursor-session-projects"
              style={{ height: Math.max(220, data.by_project.slice(0, 8).length * 36) }}
            />
            <div className="table-wrap">
              <table className="data-table compact">
                <thead>
                  <tr>
                    <th>项目</th>
                    <th>会话数</th>
                    <th>轮次数</th>
                  </tr>
                </thead>
                <tbody>
                  {data.by_project.slice(0, 12).map((row) => (
                    <tr key={row.name}>
                      <td title={row.name}>{projectLabel(row.name)}</td>
                      <td>{formatTokens(row.session_count)}</td>
                      <td>{formatTokens(row.turn_count)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        ) : (
          <p className="note">暂无项目分布数据。</p>
        )}
      </section>
    </div>
  );
}
