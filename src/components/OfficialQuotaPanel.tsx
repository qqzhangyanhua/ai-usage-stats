import { memo } from "react";
import { formatClock } from "../lib/format";
import type { OfficialQuotaDto, OfficialQuotaFreshness, OfficialQuotaRow } from "../types";
import { EmptyState } from "./EmptyState";

const FRESHNESS_LABEL: Record<OfficialQuotaFreshness, string> = {
  official: "官方",
  stale: "已过期",
  unavailable: "暂无",
};

export const OfficialQuotaPanel = memo(function OfficialQuotaPanel({
  data,
}: {
  data: OfficialQuotaDto | null;
}) {
  const rows = data?.rows ?? [];
  return (
    <article className="panel official-quota-panel">
      <div className="panel-head">
        <h2>官方额度</h2>
        <span className="muted">
          账号级订阅限额，与上方本机估计窗不是同一口径
          {data ? ` · ${data.stale_after_minutes} 分钟后标为过期` : ""}
        </span>
      </div>
      {rows.length === 0 ? (
        <EmptyState
          compact
          icon="clock"
          title={data ? "所选账号均已隐藏" : "还没有官方额度"}
          hint={data ? "在「配置显示」里打开 Codex / Claude Code / Cursor / Grok" : undefined}
        />
      ) : (
        <ul className="official-quota-list">
          {rows.map((row) => (
            <QuotaRow key={row.provider} row={row} />
          ))}
        </ul>
      )}
    </article>
  );
});

function QuotaRow({ row }: { row: OfficialQuotaRow }) {
  const tone =
    row.freshness === "official" ? "ok" : row.freshness === "stale" ? "warn" : "idle";
  return (
    <li className={`official-quota-row tone-${tone}`}>
      <div className="official-quota-head">
        <strong>{row.application}</strong>
        <em>{FRESHNESS_LABEL[row.freshness]}</em>
      </div>
      {row.windows.length === 0 ? (
        <span className="muted">{row.error ?? "尚未捕获官方额度"}</span>
      ) : (
        <div className="official-quota-windows">
          {row.windows.map((window) => {
            const percent = window.used_percent;
            return (
              <div className="official-quota-window" key={`${row.provider}-${window.kind}`}>
                <span>{window.label}</span>
                <strong>{percent == null ? "—" : `${percent.toFixed(0)}%`}</strong>
                <div className="billing-bar" aria-hidden="true">
                  <i style={{ width: `${Math.min(100, Math.max(0, percent ?? 0))}%` }} />
                </div>
                <span className="muted">
                  {window.resets_at ? `重置 ${formatClock(window.resets_at)}` : "重置时间未知"}
                </span>
              </div>
            );
          })}
        </div>
      )}
      {row.error && row.windows.length > 0 ? <span className="muted">{row.error}</span> : null}
    </li>
  );
}
