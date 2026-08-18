import { useEffect, useMemo, useState } from "react";
import { formatClock, formatTokens, projectLabel, relativeTime } from "../lib/format";
import type { CursorSessionListRow } from "../types";
import { EmptyState } from "./EmptyState";
import { ExportButton } from "./ExportButton";
import { Pagination } from "./Pagination";
import { SessionIdCell, SortArrow } from "./SessionTableParts";
import type { CursorSessionSortKey } from "./type";
import { SearchField } from "./ui/Field";
import { Select } from "./ui/Select";

const PAGE_SIZE = 20;
const ALL_PROJECTS = "__all__";

const TABLE_COLUMNS: { key: CursorSessionSortKey | "model"; label: string }[] = [
  { key: "session", label: "会话 ID" },
  { key: "project", label: "项目" },
  { key: "model", label: "模型" },
  { key: "turns", label: "轮次" },
  { key: "errors", label: "失败" },
  { key: "tools", label: "工具" },
  { key: "files", label: "文件" },
  { key: "time", label: "最近活跃" },
];

const EXPORT_HEADERS = [
  "会话ID",
  "项目",
  "模型",
  "轮次",
  "成功",
  "失败",
  "中止",
  "工具调用",
  "改动文件",
  "开始时间",
  "最近活跃",
  "原始文件",
];

type SortDir = "asc" | "desc";

function compareText(left: string, right: string): number {
  return left.localeCompare(right, "zh-CN");
}

function compareNullableTime(left: string | null, right: string | null): number {
  if (left === right) {
    return 0;
  }
  if (left == null) {
    return -1;
  }
  if (right == null) {
    return 1;
  }
  return left.localeCompare(right);
}

function sortRows(
  rows: CursorSessionListRow[],
  sortKey: CursorSessionSortKey,
  sortDir: SortDir,
): CursorSessionListRow[] {
  const sign = sortDir === "asc" ? 1 : -1;
  return [...rows].sort((left, right) => {
    let cmp = 0;
    switch (sortKey) {
      case "session":
        cmp = compareText(left.session_id, right.session_id);
        break;
      case "project":
        cmp = compareText(left.project, right.project);
        break;
      case "turns":
        cmp = left.turn_count - right.turn_count;
        break;
      case "errors":
        cmp = left.error_count - right.error_count;
        break;
      case "tools":
        cmp = left.tool_call_count - right.tool_call_count;
        break;
      case "files":
        cmp = left.files_touched - right.files_touched;
        break;
      case "time":
        cmp = compareNullableTime(left.last_seen_at, right.last_seen_at);
        break;
    }
    if (cmp === 0) {
      cmp = compareText(left.session_id, right.session_id);
    }
    return cmp * sign;
  });
}

function matchesSearch(row: CursorSessionListRow, query: string): boolean {
  if (!query) {
    return true;
  }
  const haystack = [
    row.session_id,
    row.project,
    projectLabel(row.project),
    row.models.join(" "),
    row.source_file,
  ]
    .join(" ")
    .toLowerCase();
  return haystack.includes(query);
}

function modelsLabel(models: string[]): string {
  if (models.length === 0) {
    return "—";
  }
  return models.join(", ");
}

function sessionRowToExportCells(row: CursorSessionListRow): (string | number)[] {
  return [
    row.session_id,
    row.project,
    row.models.join(", "),
    row.turn_count,
    row.success_count,
    row.error_count,
    row.aborted_count,
    row.tool_call_count,
    row.files_touched,
    formatClock(row.first_seen_at),
    formatClock(row.last_seen_at),
    row.source_file,
  ];
}

export function CursorSessionTable({
  sessions,
  selectedProject,
  onSelectProject,
}: {
  sessions: CursorSessionListRow[];
  selectedProject: string | null;
  onSelectProject: (project: string | null) => void;
}) {
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [sortKey, setSortKey] = useState<CursorSessionSortKey>("time");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [page, setPage] = useState(1);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setSearch(searchInput.trim().toLowerCase());
    }, 300);
    return () => window.clearTimeout(timer);
  }, [searchInput]);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- 筛选或排序变化时回到第一页
    setPage(1);
  }, [search, selectedProject, sortKey, sortDir]);

  const projectOptions = useMemo(() => {
    const names = [...new Set(sessions.map((row) => row.project))].sort((left, right) =>
      compareText(left, right),
    );
    return [
      { value: ALL_PROJECTS, label: "全部项目" },
      ...names.map((name) => ({ value: name, label: projectLabel(name) })),
    ];
  }, [sessions]);

  const filtered = useMemo(() => {
    return sessions.filter((row) => {
      if (selectedProject && row.project !== selectedProject) {
        return false;
      }
      return matchesSearch(row, search);
    });
  }, [sessions, selectedProject, search]);

  const sorted = useMemo(() => sortRows(filtered, sortKey, sortDir), [filtered, sortKey, sortDir]);

  const pageCount = Math.max(1, Math.ceil(sorted.length / PAGE_SIZE));
  const safePage = Math.min(page, pageCount);
  const pageRows = sorted.slice((safePage - 1) * PAGE_SIZE, safePage * PAGE_SIZE);

  function toggleSort(key: CursorSessionSortKey) {
    if (key === sortKey) {
      setSortDir((dir) => (dir === "asc" ? "desc" : "asc"));
      return;
    }
    setSortKey(key);
    setSortDir(key === "session" || key === "project" ? "asc" : "desc");
  }

  return (
    <section className="panel partition">
      <div className="panel-head">
        <h2>会话明细</h2>
        <SearchField
          value={searchInput}
          onChange={setSearchInput}
          placeholder="搜索会话 ID、项目、模型或路径"
          ariaLabel="搜索 Cursor 会话"
        />
        <span className="muted">共 {sorted.length} 个会话</span>
        <ExportButton
          filename="Cursor会话明细"
          headers={EXPORT_HEADERS}
          rows={sorted.map(sessionRowToExportCells)}
        />
      </div>
      <div className="table-scroll cursor-session-table-scroll">
        <table className="cursor-session-table">
          <thead>
            <tr>
              {TABLE_COLUMNS.map((column) => (
                <th
                  key={column.key}
                  aria-sort={
                    column.key !== "model" && sortKey === column.key
                      ? sortDir === "asc"
                        ? "ascending"
                        : "descending"
                      : "none"
                  }
                >
                  {column.key === "project" ? (
                    <div className="th-with-filter">
                      <Select
                        variant="plain"
                        ariaLabel="项目"
                        align="left"
                        value={selectedProject ?? ALL_PROJECTS}
                        options={projectOptions}
                        onChange={(project) =>
                          onSelectProject(project === ALL_PROJECTS ? null : project)
                        }
                      />
                      <button
                        type="button"
                        className="sort-th"
                        onClick={() => toggleSort("project")}
                      >
                        <SortArrow active={sortKey === "project"} dir={sortDir} />
                      </button>
                    </div>
                  ) : column.key === "model" ? (
                    column.label
                  ) : (
                    <button
                      type="button"
                      className="sort-th"
                      onClick={() => toggleSort(column.key)}
                    >
                      {column.label}
                      <SortArrow active={sortKey === column.key} dir={sortDir} />
                    </button>
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {pageRows.map((row) => (
              <tr key={`${row.source_file}-${row.session_id}`}>
                <td>
                  <SessionIdCell sessionId={row.session_id} />
                </td>
                <td title={row.project}>
                  <div className="cell-stack">
                    <span>{projectLabel(row.project)}</span>
                    <span className="muted">{row.project}</span>
                  </div>
                </td>
                <td title={modelsLabel(row.models)}>{modelsLabel(row.models)}</td>
                <td
                  title={`成功 ${row.success_count} · 失败 ${row.error_count} · 中止 ${row.aborted_count}`}
                >
                  {formatTokens(row.turn_count)}
                  {row.error_count > 0 ? (
                    <span className="muted"> / {formatTokens(row.error_count)} 失败</span>
                  ) : null}
                </td>
                <td>{formatTokens(row.error_count)}</td>
                <td>{formatTokens(row.tool_call_count)}</td>
                <td>{formatTokens(row.files_touched)}</td>
                <td
                  title={
                    row.last_seen_at
                      ? `${formatClock(row.first_seen_at)} → ${formatClock(row.last_seen_at)}`
                      : undefined
                  }
                >
                  {row.last_seen_at ? relativeTime(row.last_seen_at) : "—"}
                </td>
              </tr>
            ))}
            {pageRows.length === 0 ? (
              <tr>
                <td colSpan={8} className="analytics-empty">
                  <EmptyState
                    icon="sessions"
                    title="当前筛选条件下暂无会话"
                    hint="试试搜索会话 ID，或更换项目筛选"
                  />
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </div>
      <Pagination
        page={safePage}
        pageCount={pageCount}
        totalCount={sorted.length}
        onPageChange={setPage}
      />
    </section>
  );
}
