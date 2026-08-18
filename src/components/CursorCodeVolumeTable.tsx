import { useMemo, useState } from "react";
import { formatClock, formatTokens, relativeTime } from "../lib/format";
import { codeVolumeCommitTable } from "../lib/exportRows";
import type { CodeVolumeCommit } from "../types";
import { EmptyState } from "./EmptyState";
import { ExportButton } from "./ExportButton";
import { Pagination } from "./Pagination";

const PAGE_SIZE = 20;

function shortHash(hash: string): string {
  return hash.length > 10 ? hash.slice(0, 10) : hash;
}

export function CursorCodeVolumeTable({ commits }: { commits: CodeVolumeCommit[] }) {
  const [page, setPage] = useState(1);
  const pageCount = Math.max(1, Math.ceil(commits.length / PAGE_SIZE));
  const currentPage = Math.min(page, pageCount);
  const rows = useMemo(() => {
    const start = (currentPage - 1) * PAGE_SIZE;
    return commits.slice(start, start + PAGE_SIZE);
  }, [commits, currentPage]);
  const exportTable = codeVolumeCommitTable(commits);

  return (
    <section className="panel partition">
      <div className="panel-head">
        <h2>提交明细</h2>
        <span className="muted">共 {commits.length} 个提交</span>
        <ExportButton
          filename="Cursor代码量提交"
          headers={exportTable.headers}
          rows={exportTable.rows}
        />
      </div>
      <div className="table-scroll cursor-session-table-scroll">
        <table>
          <thead>
            <tr>
              <th>提交</th>
              <th>分支</th>
              <th>说明</th>
              <th>新增</th>
              <th>删除</th>
              <th>AI</th>
              <th>Tab</th>
              <th>时间</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={`${row.commit_hash}-${row.branch}-${row.scored_at}`}>
                <td title={row.commit_hash}>{shortHash(row.commit_hash)}</td>
                <td title={row.branch}>{row.branch || "—"}</td>
                <td title={row.commit_message}>{row.commit_message || "—"}</td>
                <td>{formatTokens(row.lines_added)}</td>
                <td>{formatTokens(row.lines_deleted)}</td>
                <td>{formatTokens(row.composer_lines_added)}</td>
                <td>{formatTokens(row.tab_lines_added)}</td>
                <td title={row.scored_at ? formatClock(row.scored_at) : undefined}>
                  {row.scored_at ? relativeTime(row.scored_at) : "—"}
                </td>
              </tr>
            ))}
            {rows.length === 0 ? (
              <tr>
                <td colSpan={8} className="analytics-empty">
                  <EmptyState icon="cursor" title="暂无提交明细" />
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </div>
      <Pagination
        page={currentPage}
        pageCount={pageCount}
        totalCount={commits.length}
        onPageChange={setPage}
      />
    </section>
  );
}
