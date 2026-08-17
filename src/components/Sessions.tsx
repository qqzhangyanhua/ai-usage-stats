import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useRef, useState } from "react";
import { Icon, sourceTone } from "../icons";
import {
  applicationLabel,
  formatCost,
  formatTokens,
  projectLabel,
  relativeTime,
} from "../lib/format";
import type { Filter, SessionPage, SessionRow, SessionSortKey, SortDir, TurnRow } from "../types";
import { EmptyState } from "./EmptyState";
import { ExportButton } from "./ExportButton";
import { KpiCard, Spark } from "./Kpi";
import { LoadingOverlay } from "./LoadingOverlay";
import { Pagination } from "./Pagination";
import { Spinner } from "./Spinner";
import { SearchField } from "./ui/Field";
import { ModelLabel } from "./VendorIcon";

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
    row.started_at,
    row.ended_at,
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
}: {
  filter: Filter;
  /** 底层数据变化（摄取、重建）时递增，用于触发重新拉取当前页 */
  revision: number;
  turns: TurnRow[];
  /** 会话明细（每轮）是否正在加载 */
  turnsLoading?: boolean;
  selected: { id: string; source: string } | null;
  onSelect: (session: { id: string; source: string }) => void;
}) {
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [sortKey, setSortKey] = useState<SessionSortKey>("tokens");
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

  const turnStats = useMemo(() => {
    const turnTotalTokens = turns.reduce((sum, turn) => sum + turn.total_tokens, 0);
    const totalCost = turns.reduce((sum, turn) => sum + (turn.cost ?? 0), 0);
    const hasCost = turns.some((turn) => turn.cost != null);
    return { totalTokens: turnTotalTokens, totalCost, hasCost };
  }, [turns]);

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
          <h2>Top 会话</h2>
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
                  <td className="mono" title={row.session_id}>
                    {row.session_id}
                  </td>
                  <td>
                    <span className={`src-pill ${sourceTone[row.source] ?? "tone-other"}`}>
                      {applicationLabel(row.source)}
                    </span>
                  </td>
                  <td title={row.project}>{projectLabel(row.project)}</td>
                  <td>
                    <span className="cell-bar">
                      <i style={{ width: `${(row.total_tokens / maxTotal) * 100}%` }} />
                    </span>
                    <span className="cell-bar-label">{formatTokens(row.total_tokens)}</span>
                  </td>
                  <td title={`${row.started_at} → ${row.ended_at}`}>
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
        <div className="panel">
          <div className="panel-head">
            <div>
              <h2>
                会话 {selected.id}（{applicationLabel(selected.source)}）每轮明细
              </h2>
              <p className="panel-note">
                共 {turns.length} 轮 · {formatTokens(turnStats.totalTokens)} Token
                {turnStats.hasCost ? ` · $${turnStats.totalCost.toFixed(4)}` : ""}
              </p>
            </div>
            <div className="export-action">
              {turns.length > 1 ? (
                <Spark values={turns.map((turn) => turn.total_tokens)} color="#8b6cff" />
              ) : null}
              <ExportButton
                label="导出明细"
                filename={`会话-${selected.id}-明细`}
                headers={[
                  "时间",
                  "模型",
                  "输入",
                  "输出",
                  "缓存读",
                  "缓存写",
                  "推理",
                  "总量",
                  "费用",
                ]}
                rows={turns.map((turn) => [
                  turn.occurred_at,
                  turn.model || "（未知）",
                  turn.input_tokens,
                  turn.output_tokens,
                  turn.cache_read_tokens,
                  turn.cache_creation_tokens,
                  turn.reasoning_tokens,
                  turn.total_tokens,
                  turn.cost ?? "",
                ])}
              />
            </div>
          </div>
          <LoadingOverlay active={turnsLoading && turns.length > 0} className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>时间</th>
                  <th>模型</th>
                  <th>输入</th>
                  <th>输出</th>
                  <th>缓存读</th>
                  <th>缓存写</th>
                  <th>推理</th>
                  <th>总量</th>
                  <th>费用</th>
                  <th>原始文件</th>
                </tr>
              </thead>
              <tbody>
                {turns.map((turn, index) => (
                  <tr key={`${turn.occurred_at}-${index}`}>
                    <td>{turn.occurred_at}</td>
                    <td>
                      <ModelLabel name={turn.model} provider={turn.provider} />
                    </td>
                    <td>{formatTokens(turn.input_tokens)}</td>
                    <td>{formatTokens(turn.output_tokens)}</td>
                    <td>{formatTokens(turn.cache_read_tokens)}</td>
                    <td>{formatTokens(turn.cache_creation_tokens)}</td>
                    <td>{formatTokens(turn.reasoning_tokens)}</td>
                    <td>
                      <strong>{formatTokens(turn.total_tokens)}</strong>
                    </td>
                    <td>
                      {formatCost(turn.cost, turn.unpriced)}
                      {turn.cost_note ? ` · ${turn.cost_note}` : ""}
                    </td>
                    <td className="mono" title={turn.source_file}>
                      {turn.source_file}
                    </td>
                  </tr>
                ))}
                {turns.length === 0 ? (
                  <tr>
                    <td colSpan={10} className="analytics-empty">
                      {turnsLoading ? (
                        <EmptyState icon="chat" title="正在加载明细…" />
                      ) : (
                        <EmptyState icon="chat" title="该会话暂无明细" />
                      )}
                    </td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </LoadingOverlay>
        </div>
      ) : null}
    </div>
  );
}
