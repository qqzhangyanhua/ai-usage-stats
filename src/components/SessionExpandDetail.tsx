import { sourceTone } from "../icons";
import {
  applicationLabel,
  formatClock,
  formatCost,
  formatTokens,
} from "../lib/format";
import type { SessionRow } from "../types";
import { SessionIdCell } from "./SessionTableParts";
import { SessionResumeCommand } from "./SessionResumeCommand";

export function SessionExpandDetail({ row }: { row: SessionRow }) {
  return (
    <div className="session-detail">
      <dl className="session-detail-meta">
        <div>
          <dt>会话 ID</dt>
          <dd>
            <SessionIdCell sessionId={row.session_id} />
          </dd>
        </div>
        <div>
          <dt>应用</dt>
          <dd>
            <span className={`src-pill ${sourceTone[row.source] ?? "tone-other"}`}>
              {applicationLabel(row.source)}
            </span>
          </dd>
        </div>
        <div>
          <dt>项目</dt>
          <dd title={row.project || undefined}>{row.project || "未标注"}</dd>
        </div>
        <div>
          <dt>模型</dt>
          <dd title={row.model || undefined}>{row.model || "—"}</dd>
        </div>
        <div>
          <dt>Token</dt>
          <dd>{formatTokens(row.total_tokens)}</dd>
        </div>
        <div>
          <dt>费用</dt>
          <dd>
            {formatCost(row.cost, row.unpriced)}
            {row.unpriced ? <span className="muted"> *</span> : null}
          </dd>
        </div>
        <div>
          <dt>开始</dt>
          <dd>{formatClock(row.started_at)}</dd>
        </div>
        <div>
          <dt>结束</dt>
          <dd>{formatClock(row.ended_at)}</dd>
        </div>
        <div className="session-detail-file">
          <dt>原始文件</dt>
          <dd className="mono" title={row.source_file}>
            {row.source_file || "—"}
          </dd>
        </div>
      </dl>
      <SessionResumeCommand source={row.source} sessionId={row.session_id} />
    </div>
  );
}
