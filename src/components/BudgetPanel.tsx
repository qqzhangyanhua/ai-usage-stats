import { useState } from "react";
import { formatUsd } from "../lib/format";
import type { BudgetStatusDto } from "../types";
import { Button } from "./ui/Button";
import { Field } from "./ui/Field";

/** 达到该百分比时进度条变色，用于直观提示预算压力（与后端提醒阈值一致）。 */
function progressTone(percent: number | null): "ok" | "warn" | "danger" {
  if (percent == null) {
    return "ok";
  }
  if (percent >= 100) {
    return "danger";
  }
  if (percent >= 80) {
    return "warn";
  }
  return "ok";
}

export function BudgetPanel({
  status,
  saving,
  onSave,
}: {
  status: BudgetStatusDto | null;
  saving: boolean;
  onSave: (monthlyUsd: number | null) => void;
}) {
  const [draft, setDraft] = useState("");
  // 用「渲染期间同步状态」模式（而非 useEffect）把服务端预算值带入草稿，
  // 避免额外一次渲染，同时保留用户在保存前的手动编辑。
  const [syncedBudget, setSyncedBudget] = useState<number | null | undefined>(undefined);
  if (status && status.monthly_budget !== syncedBudget) {
    setSyncedBudget(status.monthly_budget);
    setDraft(status.monthly_budget != null ? String(status.monthly_budget) : "");
  }

  const percentUsed = status?.percent_used ?? null;
  const percentProjected = status?.percent_projected ?? null;
  const tone = progressTone(percentUsed);
  const barWidth = percentUsed == null ? 0 : Math.min(100, percentUsed);

  function submit() {
    const trimmed = draft.trim();
    if (trimmed === "") {
      onSave(null);
      return;
    }
    const parsed = Number(trimmed);
    if (!Number.isFinite(parsed) || parsed < 0) {
      return;
    }
    onSave(parsed);
  }

  return (
    <section className="panel">
      <div className="panel-head">
        <div>
          <h2>预算与提醒</h2>
          <p className="panel-note">
            设置本月预算后，费用达到 50% / 80% / 100% 时会各弹一次系统通知（按自然月重置）。
            仅本地估算，非官方账单。
          </p>
        </div>
        <div className="row-actions">
          <Field
            label="月度预算（美元）"
            type="number"
            min="0"
            step="any"
            placeholder="不设置则不提醒"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
          />
          <Button variant="accent" disabled={saving} onClick={submit}>
            {saving ? "保存中…" : "保存"}
          </Button>
        </div>
      </div>
      {status ? (
        <div className="budget-status">
          <div className="budget-bar" aria-hidden="true">
            <i className={`budget-bar-fill ${tone}`} style={{ width: `${barWidth}%` }} />
          </div>
          <dl className="budget-metrics">
            <div>
              <dt>本月（{status.month}）已花费</dt>
              <dd>{formatUsd(status.month_to_date_cost, status.unpriced)}</dd>
            </div>
            <div>
              <dt>预算进度</dt>
              <dd>{percentUsed != null ? `${percentUsed.toFixed(1)}%` : "未设置预算"}</dd>
            </div>
            <div>
              <dt>预计月末费用</dt>
              <dd>
                {status.projected_month_cost != null
                  ? formatUsd(status.projected_month_cost, status.unpriced)
                  : "—"}
                {percentProjected != null ? `（${percentProjected.toFixed(0)}%）` : ""}
              </dd>
            </div>
            <div>
              <dt>本月进度</dt>
              <dd>
                第 {status.days_elapsed} / {status.days_in_month} 天
              </dd>
            </div>
          </dl>
        </div>
      ) : null}
    </section>
  );
}
