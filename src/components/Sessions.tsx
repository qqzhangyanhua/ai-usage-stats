import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState, type MouseEvent } from "react";
import { Icon, sourceTone } from "../icons";
import {
  applicationLabel,
  formatClock,
  formatTokens,
  projectLabel,
  relativeTime,
} from "../lib/format";
import type { Filter, SessionPage, SessionRow, SessionSortKey, SortDir, TurnRow } from "../types";
import { EmptyState } from "./EmptyState";
import { ExportButton } from "./ExportButton";
import { KpiCard } from "./Kpi";
import { LoadingOverlay } from "./LoadingOverlay";
import { Pagination } from "./Pagination";
import { SessionTurns } from "./SessionTurns";
import { Spinner } from "./Spinner";
import { Button } from "./ui/Button";
import { SearchField } from "./ui/Field";

const PAGE_SIZE = 20;
const SEARCH_DEBOUNCE_MS = 300;
const EXPORT_ROW_LIMIT = 20000;

const SORT_COLUMNS: { key: SessionSortKey; label: string }[] = [
  { key: "session", label: "会话" },
  { key: "application", label: "应用" },
  { key: "project", label: "项目" },
  { key: "tokens", label: "token" },
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
  revision,
  turns,
  turnsLoading = false,
  selected,
  onSelect,
  onFilterChange,
}: {
  filter: Filter;
  /** 底层数据变化（摄取、重建）时递增，用于触发重新拉取当前页 */
  revision: number;
  turns: TurnRow[];
  /** 会话明细（每轮）是否正在加载 */
  turnsLoading?: boolean;
  selected: { id: string; source: string } | null;
  onSelect: (session: { id: string; source: string }) => void;
  onFilterChange: (filter: Filter) => void;
}) {
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
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
  const requestGeneration = useRef(0);

  useEffect(() => {
    const id = window.setTimeout(() => {
      setSearch(searchInput.trim());
      setPage(1);
    }, SEARCH_DEBOUNCE_MS);
    return () => window.clearTimeout(id);
  }, [searchInput]);

  useEffect(() => {
    setPage(1);
  }, [filter]);

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
      },
    })
      .then((result) => {
        if (generation === requestGeneration.current) {
          setPageData(result);
        }
      })
      .catch(() => {
        // 忽略单次请求失败，保留上一次成功的数据
      })
      .finally(() => {
        if (generation === requestGeneration.current) {
          setLoading(false);
        }
      });
    // revision 变化即代表底层数据可能已更新，需要重新拉取
  }, [filter, revision, search, sortKey, sortDir, page]);

  const { rows, total, totalTokens, lastEnded } = pageData;
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const averageTokens = total > 0 ? totalTokens / total : 0;
  const maxTotal = Math.max(1, ...rows.map((row) => row.total_tokens));

  function toggleSort(key: SessionSortKey) {
    if (key === sortKey) {
      setSortDir((dir) => (dir === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir(key === "tokens" ? "desc" : "asc");
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
            placeholder="搜索会话 / 项目 / 模型…"
            value={searchInput}
            onChange={setSearchInput}
            ariaLabel="搜索会话"
          />
          <span className="muted">
            共 {total} 个会话{search ? `（已筛选）` : ""}
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
                    <button className="sort-th" onClick={() => toggleSort(column.key)}>
                      {column.label}
                      <Icon
                        name="chevron"
                        size={11}
                        className={
                          sortKey === column.key
                            ? sortDir === "asc"
                              ? "sort-arrow asc"
                              : "sort-arrow desc"
                            : "sort-arrow idle"
                        }
                      />
                    </button>
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
                    <FilterChip
                      label={applicationLabel(row.source)}
                      title={`筛选应用：${applicationLabel(row.source)}`}
                      active={isSoleFilter(filter.sources, row.source)}
                      className={`src-pill ${sourceTone[row.source] ?? "tone-other"}`}
                      onPick={() =>
                        onFilterChange({
                          ...filter,
                          sources: toggleSoleFilter(filter.sources, row.source),
                        })
                      }
                    />
                  </td>
                  <td>
                    <FilterChip
                      label={projectLabel(row.project)}
                      title={`筛选项目：${projectLabel(row.project)}`}
                      active={isSoleFilter(filter.projects, row.project)}
                      className="project-chip"
                      onPick={() =>
                        onFilterChange({
                          ...filter,
                          projects: toggleSoleFilter(filter.projects, row.project),
                        })
                      }
                    />
                  </td>
                  <td>
                    <span className="cell-bar">
                      <i style={{ width: `${(row.total_tokens / maxTotal) * 100}%` }} />
                    </span>
                    <span className="cell-bar-label">{formatTokens(row.total_tokens)}</span>
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
                  <td colSpan={6} className="analytics-empty">
                    {loading ? (
                      <EmptyState icon="sessions" title="正在加载会话…" />
                    ) : (
                      <EmptyState
                        icon="sessions"
                        title={search ? "没有匹配的会话" : "当前筛选条件下暂无会话"}
                        hint={search ? "试试更换关键词或清空搜索条件" : "调整筛选条件后再试试"}
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

function isSoleFilter(selected: string[], value: string): boolean {
  return selected.length === 1 && selected[0] === value;
}

function toggleSoleFilter(selected: string[], value: string): string[] {
  return isSoleFilter(selected, value) ? [] : [value];
}

function FilterChip({
  label,
  title,
  active,
  className,
  onPick,
}: {
  label: string;
  title: string;
  active: boolean;
  className: string;
  onPick: () => void;
}) {
  return (
    <button
      type="button"
      className={`${className}${active ? " is-active" : ""}`}
      title={title}
      onClick={(event: MouseEvent<HTMLButtonElement>) => {
        event.stopPropagation();
        onPick();
      }}
    >
      {label}
    </button>
  );
}

function SessionIdCell({ sessionId }: { sessionId: string }) {
  const [copied, setCopied] = useState(false);

  async function copyId(event: MouseEvent<HTMLButtonElement>) {
    event.stopPropagation();
    try {
      await navigator.clipboard.writeText(sessionId);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  }

  return (
    <div className="session-id-cell">
      <span className="mono" title={sessionId}>
        {sessionId}
      </span>
      <Button
        variant="icon"
        className={copied ? "table-icon-btn is-copied" : "table-icon-btn"}
        onClick={copyId}
        title={copied ? "已复制" : "复制会话 ID"}
        aria-label={copied ? "已复制会话 ID" : "复制会话 ID"}
      >
        <Icon name={copied ? "check" : "copy"} size={12} />
      </Button>
    </div>
  );
}
