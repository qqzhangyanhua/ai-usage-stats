import { Icon, sourceTone } from "../icons";
import {
  applicationLabel,
  formatClock,
  formatCost,
  formatTokens,
  projectLabel,
  relativeTime,
} from "../lib/format";
import type { SessionRow } from "../types";
import { SessionExpandDetail } from "./SessionExpandDetail";
import { SessionIdCell } from "./SessionTableParts";

export function SessionTableRow({
  row,
  selected,
  maxTotal,
  colSpan,
  onToggle,
}: {
  row: SessionRow;
  selected: boolean;
  maxTotal: number;
  colSpan: number;
  onToggle: () => void;
}) {
  return (
    <>
      <tr
        className={selected ? "clickable selected" : "clickable"}
        aria-expanded={selected}
        onClick={onToggle}
      >
        <td className="session-expand-td">
          <Icon
            name="chevron"
            size={11}
            className={selected ? "session-expand-caret is-open" : "session-expand-caret"}
          />
        </td>
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
      {selected ? (
        <tr className="session-detail-row">
          <td colSpan={colSpan}>
            <SessionExpandDetail row={row} />
          </td>
        </tr>
      ) : null}
    </>
  );
}
