import { useMemo, useState } from "react";
import {
  breakdownBarOption,
  cursorSessionDailyOption,
  donutOption,
  modelPalette,
} from "../lib/chartTheme";
import type { ResolvedTheme } from "../hooks/useTheme";
import { cursorSessionProjectTable, cursorSessionToolTable } from "../lib/exportRows";
import {
  formatClock,
  formatCompact,
  formatTokens,
  projectLabel,
  relativeTime,
} from "../lib/format";
import type { CursorSessionSummaryDto } from "../types";
import { CursorSessionTable } from "./CursorSessionTable";
import { DonutChart } from "./DonutChart";
import { EmptyState } from "./EmptyState";
import { ExportButton } from "./ExportButton";
import { ExportableChart } from "./ExportableChart";
import { KpiCard, LegendRow } from "./Kpi";

function emptySummary(): CursorSessionSummaryDto {
  return {
    as_of: null,
    session_count: 0,
    turn_count: 0,
    error_rate: null,
    active_project_count: 0,
    by_project: [],
    by_model: [],
    top_tools: [],
    daily: [],
    sessions: [],
  };
}

export function CursorSessionPanel({
  summary,
  theme,
}: {
  summary: CursorSessionSummaryDto | null;
  theme: ResolvedTheme;
}) {
  const data = summary ?? emptySummary();
  const [selectedProject, setSelectedProject] = useState<string | null>(null);

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

  const modelOption = useMemo(() => {
    const slices = data.by_model.map((row, index) => ({
      name: row.name,
      value: row.session_count,
      color: modelPalette[index % modelPalette.length],
    }));
    return donutOption(slices, theme);
  }, [data.by_model, theme]);

  const toolOption = useMemo(() => {
    const top = data.top_tools.slice(0, 10);
    const labels = top.map((row) => row.name).reverse();
    const values = top.map((row) => row.call_count).reverse();
    return breakdownBarOption(labels, values, theme);
  }, [data.top_tools, theme]);

  const modelTotal = data.by_model.reduce((sum, row) => sum + row.session_count, 0);

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

      <CursorSessionTable
        sessions={data.sessions}
        selectedProject={selectedProject}
        onSelectProject={setSelectedProject}
      />

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

      <div className="split-2">
        <section className="panel partition">
          <div className="panel-head">
            <h2>按模型</h2>
          </div>
          {data.by_model.length > 0 ? (
            <div className="donut-wrap">
              <DonutChart option={modelOption} centerValue={formatCompact(modelTotal)} />
              <div className="legend-col">
                {data.by_model.slice(0, 8).map((row, index) => (
                  <LegendRow
                    key={row.name}
                    color={modelPalette[index % modelPalette.length]}
                    label={row.name}
                    value={`${formatTokens(row.session_count)} 会话`}
                  />
                ))}
              </div>
            </div>
          ) : (
            <p className="note">暂无模型 enrich 数据（纯问答或未关联 ai_code_hashes）。</p>
          )}
        </section>

        <section className="panel partition">
          <div className="panel-head">
            <h2>工具调用 Top N</h2>
            <ExportButton
              filename="Cursor会话工具"
              headers={cursorSessionToolTable(data).headers}
              rows={cursorSessionToolTable(data).rows}
            />
          </div>
          {data.top_tools.length > 0 ? (
            <ExportableChart
              option={toolOption}
              filename="cursor-session-tools"
              style={{ height: Math.max(220, data.top_tools.slice(0, 10).length * 36) }}
            />
          ) : (
            <p className="note">暂无工具调用记录。</p>
          )}
        </section>
      </div>

      <section className="panel partition">
        <div className="panel-head">
          <h2>按项目</h2>
          <span className="muted">点击项目可筛选上方会话明细</span>
          <ExportButton
            filename="Cursor会话项目"
            headers={cursorSessionProjectTable(data).headers}
            rows={cursorSessionProjectTable(data).rows}
          />
        </div>
        {data.by_project.length > 0 ? (
          <>
            <ExportableChart
              option={projectOption}
              filename="cursor-session-projects"
              style={{ height: Math.max(220, data.by_project.slice(0, 8).length * 36) }}
            />
            <div className="table-scroll cursor-session-table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>项目</th>
                    <th>会话数</th>
                    <th>轮次</th>
                    <th>失败</th>
                    <th>文件</th>
                    <th>最近活跃</th>
                  </tr>
                </thead>
                <tbody>
                  {data.by_project.map((row) => (
                    <tr
                      key={row.name}
                      className={selectedProject === row.name ? "clickable selected" : "clickable"}
                      onClick={() =>
                        setSelectedProject((current) => (current === row.name ? null : row.name))
                      }
                    >
                      <td title={row.name}>
                        <div className="cell-stack">
                          <span>{projectLabel(row.name)}</span>
                          <span className="muted">{row.name}</span>
                        </div>
                      </td>
                      <td>{formatTokens(row.session_count)}</td>
                      <td>{formatTokens(row.turn_count)}</td>
                      <td>{formatTokens(row.error_count)}</td>
                      <td>{formatTokens(row.files_touched)}</td>
                      <td title={row.last_seen_at ? formatClock(row.last_seen_at) : undefined}>
                        {row.last_seen_at ? relativeTime(row.last_seen_at) : "—"}
                      </td>
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
