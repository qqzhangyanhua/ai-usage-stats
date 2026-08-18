import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { sourceTone } from "../icons";
import {
  applicationLabel,
  formatClock,
  formatCost,
  formatTokens,
  projectLabel,
  relativeTime,
} from "../lib/format";
import type {
  Filter,
  FilterOptions,
  SessionPage,
  SessionRow,
  SessionSortKey,
  SortDir,
  TurnRow,
} from "../types";
import { EmptyState } from "./EmptyState";
import { ExportButton } from "./ExportButton";
import { KpiCard } from "./Kpi";
import { LoadingOverlay } from "./LoadingOverlay";
import { Pagination } from "./Pagination";
import { SessionIdCell, SortArrow, SortButton } from "./SessionTableParts";
import { SessionTurns } from "./SessionTurns";
import { Spinner } from "./Spinner";
import { SearchField } from "./ui/Field";
import { Select } from "./ui/Select";

const ALL_APPS = "__all__";
const ALL_PROJECTS = "__all__";

const PAGE_SIZE = 20;
const EXPORT_ROW_LIMIT = 20000;

const SORT_COLUMNS: { key: SessionSortKey; label: string }[] = [
  { key: "session", label: "会话" },
  { key: "application", label: "应用" },
  { key: "project", label: "项目" },
  { key: "model", label: "模型" },
  { key: "tokens", label: "token" },
  { key: "cost", label: "费用" },
  { key: "time", label: "起止" },
];

const EXPORT_HEADERS = [
  "会话ID",
  "应用",
  "项目",
  "模型",
  "Token",
  "开始时间",
  "结束时间",
  "费用",
  "原始文件",
];

function sessionRowToExportCells(row: SessionRow): (string | number)[] {
  return [
    row.session_id,
    applicationLabel(row.source),
    projectLabel(row.project),
    row.model,
    row.total_tokens,
    formatClock(row.started_at),
    formatClock(row.ended_at),
    row.cost ?? "",
    row.source_file,
  ];
}

export function Sessions({
  filter,
  options,
  revision,
  turns,
  turnsLoading = false,
  selected,
  onSelect,
  onFilterChange,
  onError,
}: {
  filter: Filter;
  options: FilterOptions;
  /** 底层数据变化（摄取、重建）时递增，用于触发重新拉取当前页 */
  revision: number;
  turns: TurnRow[];
  /** 会话明细（每轮）是否正在加载 */
  turnsLoading?: boolean;
  selected: { id: string; source: string } | null;
  onSelect: (session: { id: string; source: string }) => void;
  onFilterChange: (filter: Filter) => void;
  onError?: (error: unknown) => void;
}) {
  const [sortKey, setSortKey] = useState<SessionSortKey>("time");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [page, setPage] = useState(1);
  const [pageData, setPageData] = useState<SessionPage>({
    rows: [],
    total: 0,
    totalTokens: 0,
    lastEnded: null,
  });
  const [loading, setLoading] = useState(false);
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const requestGeneration = useRef(0);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setSearch(searchInput.trim());
    }, 300);
    return () => window.clearTimeout(timer);
  }, [searchInput]);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- 筛选或搜索变化时回到第一页
    setPage(1);
  }, [filter, search]);

  useEffect(() => {
    const generation = ++requestGeneration.current;
    // eslint-disable-next-line react-hooks/set-state-in-effect -- 标准的“发起请求前先置 loading”写法
    setLoading(true);
    invoke<SessionPage>("get_sessions_page", {
      query: {
        filter,
        search: search || null,
        sortBy: sortKey,
        sortDir,
        page,
        pageSize: PAGE_SIZE,
        includeCost: true,
      },
    })
      .then((result) => {
        if (generation === requestGeneration.current) {
          setPageData(result);
        }
      })
      .catch((error) => {
        // 保留上一次成功的数据，但仍需要把失败告知用户，而不是完全静默
        if (generation === requestGeneration.current) {
          onError?.(error);
        }
      })
      .finally(() => {
        if (generation === requestGeneration.current) {
          setLoading(false);
        }
      });
    // revision 变化即代表底层数据可能已更新，需要重新拉取
  }, [filter, revision, search, sortKey, sortDir, page, onError]);

  const { rows, total, totalTokens, lastEnded } = pageData;
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const averageTokens = total > 0 ? totalTokens / total : 0;
  const maxTotal = Math.max(1, ...rows.map((row) => row.total_tokens));

  function toggleSort(key: SessionSortKey) {
    if (key === sortKey) {
      setSortDir((dir) => (dir === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir(key === "tokens" || key === "cost" ? "desc" : "asc");
    }
    setPage(1);
  }

  async function fetchAllMatchingRows(): Promise<(string | number)[][]> {
    const result = await invoke<SessionPage>("get_sessions_page", {
      query: {
        filter,
        search: search || null,
        sortBy: sortKey,
        sortDir,
        page: 1,
        pageSize: Math.min(Math.max(total, 1), EXPORT_ROW_LIMIT),
        includeCost: true,
      },
    });
    return result.rows.map(sessionRowToExportCells);
  }

  return (
    <div className="stack">
      <section className="kpi-row">
        <KpiCard icon="sessions" tone="purple" label="会话数" value={formatTokens(total)} />
        <KpiCard icon="tokens" tone="cyan" label="合计 Token" value={formatTokens(totalTokens)} />
        <KpiCard
          icon="daily"
          tone="blue"
          label="平均每会话 Token"
          value={formatTokens(Math.round(averageTokens))}
        />
        <KpiCard
          icon="clock"
          tone="orange"
          label="最近一次会话"
          value={lastEnded ? relativeTime(lastEnded) : "—"}
        />
      </section>

      <div className="panel">
        <div className="panel-head">
          <h2>会话管理</h2>
          <SearchField
            value={searchInput}
            onChange={setSearchInput}
            placeholder="搜索会话、项目、模型或路径"
            ariaLabel="搜索会话"
          />
          <span className="muted">
            共 {total} 个会话
            {loading ? (
              <span className="inline-loading">
                <Spinner size={12} />
                加载中…
              </span>
            ) : null}
          </span>
          <ExportButton
            filename="会话列表"
            headers={EXPORT_HEADERS}
            getRows={fetchAllMatchingRows}
          />
        </div>
        <LoadingOverlay active={loading && rows.length > 0} className="table-scroll">
          <table>
            <thead>
              <tr>
                {SORT_COLUMNS.map((column) => (
                  <th
                    key={column.key}
                    aria-sort={
                      sortKey === column.key
                        ? sortDir === "asc"
                          ? "ascending"
                          : "descending"
                        : "none"
                    }
                  >
                    {column.key === "application" ? (
                      <div className="th-with-filter">
                        <Select
                          variant="plain"
                          ariaLabel="应用"
                          align="left"
                          value={filter.sources.length === 1 ? filter.sources[0] : ALL_APPS}
                          options={[
                            { value: ALL_APPS, label: "全部应用" },
                            ...options.sources.map((source) => ({
                              value: source,
                              label: applicationLabel(source),
                            })),
                          ]}
                          onChange={(source) =>
                            onFilterChange({
                              ...filter,
                              sources: source === ALL_APPS ? [] : [source],
                            })
                          }
                        />
                        <SortButton
                          active={sortKey === column.key}
                          dir={sortDir}
                          onClick={() => toggleSort(column.key)}
                        />
                      </div>
                    ) : column.key === "project" ? (
                      <div className="th-with-filter">
                        <Select
                          variant="plain"
                          ariaLabel="项目"
                          align="left"
                          value={filter.projects.length === 1 ? filter.projects[0] : ALL_PROJECTS}
                          options={[
                            { value: ALL_PROJECTS, label: "全部项目" },
                            ...options.projects.map((project) => ({
                              value: project,
                              label: projectLabel(project),
                            })),
                          ]}
                          onChange={(project) =>
                            onFilterChange({
                              ...filter,
                              projects: project === ALL_PROJECTS ? [] : [project],
                            })
                          }
                        />
                        <SortButton
                          active={sortKey === column.key}
                          dir={sortDir}
                          onClick={() => toggleSort(column.key)}
                        />
                      </div>
                    ) : (
                      <button className="sort-th" onClick={() => toggleSort(column.key)}>
                        {column.label}
                        <SortArrow active={sortKey === column.key} dir={sortDir} />
                      </button>
                    )}
                  </th>
                ))}
                <th>原始文件</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr
                  key={`${row.source}-${row.session_id}`}
                  className={
                    selected?.id === row.session_id && selected.source === row.source
                      ? "clickable selected"
                      : "clickable"
                  }
                  onClick={() => onSelect({ id: row.session_id, source: row.source })}
                >
                  <td>
                    <SessionIdCell sessionId={row.session_id} />
                  </td>
                  <td>
                    <span className={`src-pill ${sourceTone[row.source] ?? "tone-other"}`}>
                      {applicationLabel(row.source)}
                    </span>
                  </td>
                  <td title={row.project}>{projectLabel(row.project)}</td>
                  <td title={row.model || undefined}>{row.model || "—"}</td>
                  <td>
                    <span className="cell-bar">
                      <i style={{ width: `${(row.total_tokens / maxTotal) * 100}%` }} />
                    </span>
                    <span className="cell-bar-label">{formatTokens(row.total_tokens)}</span>
                  </td>
                  <td title={row.unpriced ? "部分轮次单价未配置" : undefined}>
                    {formatCost(row.cost, row.unpriced)}
                    {row.unpriced ? <span className="muted"> *</span> : null}
                  </td>
                  <td title={`${formatClock(row.started_at)} → ${formatClock(row.ended_at)}`}>
                    {relativeTime(row.started_at)} → {relativeTime(row.ended_at)}
                  </td>
                  <td className="mono" title={row.source_file}>
                    {row.source_file}
                  </td>
                </tr>
              ))}
              {rows.length === 0 ? (
                <tr>
                  <td colSpan={8} className="analytics-empty">
                    {loading ? (
                      <EmptyState icon="sessions" title="正在加载会话…" />
                    ) : (
                      <EmptyState
                        icon="sessions"
                        title="当前筛选条件下暂无会话"
                        hint="试试搜索会话，或更换应用、项目筛选"
                      />
                    )}
                  </td>
                </tr>
              ) : null}
            </tbody>
          </table>
        </LoadingOverlay>
        <Pagination page={page} pageCount={pageCount} totalCount={total} onPageChange={setPage} />
      </div>
      {selected ? (
        <SessionTurns
          sessionId={selected.id}
          source={selected.source}
          sourceLabel={applicationLabel(selected.source)}
          turns={turns}
          turnsLoading={turnsLoading}
        />
      ) : null}
    </div>
  );
}
