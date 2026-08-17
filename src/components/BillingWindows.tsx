import { memo, useEffect, useState } from "react";
import { sourceTone } from "../icons";
import {
  applicationLabel,
  formatCompact,
  formatHoursMinutes,
  formatUsd,
  formatWindowClock,
} from "../lib/format";
import type { BillingWindowDto, BillingWindowsDto } from "../types";
import { EmptyState } from "./EmptyState";

const TICK_MS = 60_000;

export const BillingWindows = memo(function BillingWindows({
  data,
}: {
  data: BillingWindowsDto | null;
}) {
  const [, setTick] = useState(0);
  useEffect(() => {
    const id = window.setInterval(() => {
      setTick((value) => value + 1);
    }, TICK_MS);
    return () => window.clearInterval(id);
  }, []);

  const current = data?.current ?? [];
  const recent = data?.recent ?? [];
  const hours = data?.window_hours ?? 5;

  return (
    <article className="panel billing-panel">
      <div className="panel-head">
        <h2>5 小时计费窗</h2>
        <span className="muted">由本地时间戳估计，非官方配额</span>
      </div>
      {current.length === 0 ? (
        <EmptyState compact icon="clock" title="当前没有进行中的 5 小时窗" />
      ) : (
        <div className="billing-current">
          {current.map((window) => (
            <WindowCard key={`${window.source}-${window.start}`} window={window} hours={hours} />
          ))}
        </div>
      )}
      {recent.length > 0 ? (
        <ol className="billing-recent">
          {recent.map((window) => (
            <li key={`${window.source}-${window.start}`}>
              <span className={`src-ico ${sourceTone[window.source] ?? "tone-other"}`}>
                {applicationLabel(window.source).slice(0, 1)}
              </span>
              <span className="billing-recent-name">{window.application}</span>
              <span className="muted">
                {formatWindowClock(window.start)} – {formatWindowClock(window.end)}
              </span>
              <span className="billing-recent-tokens">{formatCompact(window.total_tokens)}</span>
              <span className="billing-recent-cost">{formatUsd(window.cost, window.unpriced)}</span>
            </li>
          ))}
        </ol>
      ) : null}
    </article>
  );
});

function WindowCard({ window, hours }: { window: BillingWindowDto; hours: number }) {
  const remaining = liveRemainingMinutes(window);
  const elapsed = liveElapsedMinutes(window, hours);
  const span = hours * 60;
  const progress = Math.min(100, (elapsed / Math.max(span, 1)) * 100);

  return (
    <div className="billing-card">
      <div className="billing-card-head">
        <span className={`src-ico ${sourceTone[window.source] ?? "tone-other"}`}>
          {applicationLabel(window.source).slice(0, 1)}
        </span>
        <strong>{window.application}</strong>
        <em>{remaining != null ? `剩余 ${formatHoursMinutes(remaining)}` : "已结束"}</em>
      </div>
      <div className="billing-bar" aria-hidden="true">
        <i style={{ width: `${progress}%` }} />
      </div>
      <div className="billing-meta muted">
        {formatWindowClock(window.start)} – {formatWindowClock(window.end)} · 已过{" "}
        {formatHoursMinutes(elapsed)} / {hours}h
      </div>
      <dl className="billing-metrics">
        <div>
          <dt>本窗 Token</dt>
          <dd>{formatCompact(window.total_tokens)}</dd>
        </div>
        <div>
          <dt>费用</dt>
          <dd>{formatUsd(window.cost, window.unpriced)}</dd>
        </div>
        <div>
          <dt>燃烧速率</dt>
          <dd>{formatBurn(window)}</dd>
        </div>
        <div>
          <dt>窗末预测</dt>
          <dd>{formatProjection(window)}</dd>
        </div>
      </dl>
    </div>
  );
}

function liveRemainingMinutes(window: BillingWindowDto): number | null {
  if (!window.is_active) {
    return null;
  }
  return Math.max(0, (Date.parse(window.end) - Date.now()) / 60_000);
}

function liveElapsedMinutes(window: BillingWindowDto, hours: number): number {
  const elapsed = (Date.now() - Date.parse(window.start)) / 60_000;
  return Math.min(hours * 60, Math.max(0, elapsed));
}

function formatBurn(window: BillingWindowDto): string {
  if (!window.burn) {
    return "—";
  }
  const tokens = `${formatCompact(Math.round(window.burn.tokens_per_minute))}/分`;
  if (window.burn.cost_per_hour == null) {
    return tokens;
  }
  return `${tokens} · ${formatUsd(window.burn.cost_per_hour, false)}/时`;
}

function formatProjection(window: BillingWindowDto): string {
  if (!window.projection) {
    return "—";
  }
  const tokens = formatCompact(window.projection.total_tokens);
  if (window.projection.cost == null) {
    return tokens;
  }
  return `${tokens} · ${formatUsd(window.projection.cost, window.unpriced)}`;
}
