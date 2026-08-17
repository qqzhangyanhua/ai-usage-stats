import { memo, useEffect, useState } from "react";
import { sourceTone } from "../icons";
import {
  applicationLabel,
  formatCompact,
  formatHoursMinutes,
  formatUsd,
} from "../lib/format";
import type { BillingWindowDto, BillingWindowsDto } from "../types";

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
  const hours = data?.window_hours ?? 5;

  return (
    <article className="panel billing-panel">
      <span className="billing-label" title="由本地时间戳估计，非官方配额">
        5 小时窗
      </span>
      {current.length === 0 ? (
        <span className="muted">当前没有进行中的窗口</span>
      ) : (
        current.map((window) => (
          <WindowRow key={`${window.source}-${window.start}`} window={window} hours={hours} />
        ))
      )}
    </article>
  );
});

function WindowRow({ window, hours }: { window: BillingWindowDto; hours: number }) {
  const remaining = liveRemainingMinutes(window);
  const elapsed = liveElapsedMinutes(window, hours);
  const span = hours * 60;
  const progress = Math.min(100, (elapsed / Math.max(span, 1)) * 100);

  return (
    <div className="billing-row">
      <span className={`src-ico ${sourceTone[window.source] ?? "tone-other"}`}>
        {applicationLabel(window.source).slice(0, 1)}
      </span>
      <strong>{window.application}</strong>
      <em>{remaining != null ? `剩余 ${formatHoursMinutes(remaining)}` : "已结束"}</em>
      <div className="billing-bar" aria-hidden="true">
        <i style={{ width: `${progress}%` }} />
      </div>
      <span className="billing-stat">{formatCompact(window.total_tokens)}</span>
      <span className="billing-stat">{formatUsd(window.cost, window.unpriced)}</span>
      <span className="billing-stat">{formatBurn(window)}</span>
      <span className="billing-stat muted">{formatProjection(window)}</span>
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
