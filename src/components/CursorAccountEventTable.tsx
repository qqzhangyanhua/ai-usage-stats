import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { formatClock, formatTokens, relativeTime } from "../lib/format";
import { cursorAccountEventTable } from "../lib/exportRows";
import type { CursorAccountEventPage, CursorAccountEventRow, SortDir } from "../types";
import { EmptyState } from "./EmptyState";
import { ExportButton } from "./ExportButton";
import { LoadingOverlay } from "./LoadingOverlay";
import { Pagination } from "./Pagination";
import { SortArrow } from "./SessionTableParts";
import { Spinner } from "./Spinner";

const PAGE_SIZE = 20;
const EXPORT_ROW_LIMIT = 20_000;

export function CursorAccountEventTable({
  revision,
  onError,
}: {
  revision: number | string;
  onError?: (error: unknown) => void;
}) {
  const [page, setPage] = useState(1);
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [data, setData] = useState<CursorAccountEventPage>({ rows: [], total: 0 });
  const [loading, setLoading] = useState(false);
  const generationRef = useRef(0);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- 排序或缓存刷新时回到第一页
    setPage(1);
  }, [sortDir, revision]);

  useEffect(() => {
    const generation = ++generationRef.current;
    // eslint-disable-next-line react-hooks/set-state-in-effect -- 标准的“发起请求前先置 loading”写法
    setLoading(true);
    invoke<CursorAccountEventPage>("get_cursor_account_events_page", {
      query: { page, pageSize: PAGE_SIZE, sortDir },
    })
      .then((next) => {
        if (generation === generationRef.current) {
          setData(next);
        }
      })
      .catch((error: unknown) => {
        if (generation === generationRef.current) {
          onError?.(error);
        }
      })
      .finally(() => {
        if (generation === generationRef.current) {
          setLoading(false);
        }
      });
  }, [page, sortDir, revision, onError]);

  const pageCount = Math.max(1, Math.ceil(data.total / PAGE_SIZE));

  async function fetchAllRows(): Promise<(string | number)[][]> {
    const result = await invoke<CursorAccountEventPage>("get_cursor_account_events_page", {
      query: {
        page: 1,
        pageSize: Math.min(Math.max(data.total, 1), EXPORT_ROW_LIMIT),
        sortDir,
      },
    });
    return cursorAccountEventTable(result.rows).rows;
  }

  return (
    <section className="panel partition">
      <div className="panel-head">
        <h2>账号事件</h2>
        <span className="muted">
          云端账号事件，对不上本机会话
          {loading ? (
            <span className="inline-loading">
              <Spinner size={12} />
              加载中…
            </span>
          ) : null}
        </span>
        <ExportButton
          filename="Cursor账号事件"
          headers={cursorAccountEventTable([]).headers}
          getRows={fetchAllRows}
        />
      </div>
      <LoadingOverlay active={loading && data.rows.length > 0} className="table-scroll">
        <table>
          <thead>
            <tr>
              <th>
                <button
                  type="button"
                  className="sort-th"
                  onClick={() => setSortDir((dir) => (dir === "desc" ? "asc" : "desc"))}
                >
                  时间
                  <SortArrow active dir={sortDir} />
                </button>
              </th>
              <th>模型</th>
              <th>输入</th>
              <th>输出</th>
              <th>缓存读</th>
              <th>缓存写</th>
              <th>总量</th>
              <th>类型</th>
            </tr>
          </thead>
          <tbody>
            {data.rows.map((row, index) => (
              <EventRow key={`${row.occurred_at}-${row.model}-${index}`} row={row} />
            ))}
            {data.rows.length === 0 ? (
              <tr>
                <td colSpan={8} className="analytics-empty">
                  {loading ? (
                    <EmptyState icon="cursor" title="正在加载事件…" />
                  ) : (
                    <EmptyState icon="cursor" title="暂无账号事件" hint="刷新账号用量后再看这里。" />
                  )}
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </LoadingOverlay>
      <Pagination page={page} pageCount={pageCount} totalCount={data.total} onPageChange={setPage} />
    </section>
  );
}

function EventRow({ row }: { row: CursorAccountEventRow }) {
  return (
    <tr>
      <td title={formatClock(row.occurred_at)}>{relativeTime(row.occurred_at)}</td>
      <td>{row.model || "—"}</td>
      <td>{formatTokens(row.input_tokens)}</td>
      <td>{formatTokens(row.output_tokens)}</td>
      <td>{formatTokens(row.cache_read_tokens)}</td>
      <td>{formatTokens(row.cache_creation_tokens)}</td>
      <td>{formatTokens(row.total_tokens)}</td>
      <td>{row.is_headless ? "后台" : "交互"}</td>
    </tr>
  );
}
