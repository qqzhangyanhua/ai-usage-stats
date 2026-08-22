/**
 * 额度节奏：已用百分比 vs 计费窗已过百分比。
 *
 * 光看「已用 60%」看不出好坏——窗口刚开就 60% 是要超，快到期才 60% 是有富余。
 * 两者相减就是节奏差，正数表示烧得比时间快。
 *
 * 窗口总长由 `kind` 推断：这是展示层的映射，各家接口给的 kind 命名已经把周期写在里面了。
 * 推不出来的（Cursor 的计费周期、OpenCode 的滚动窗）返回 null，不显示节奏，
 * 而不是瞎猜一个长度把结论也带偏。
 */
const MINUTE = 60 * 1000;

const FIVE_HOURS = 5 * 60;
const DAY = 24 * 60;
const WEEK = 7 * DAY;
const MONTH = 30 * DAY;

/** 按 kind 里的周期词推断窗口总长（分钟）。顺序有意义：先长后短，避免 "weekly" 被 "day" 误伤。 */
export function windowMinutes(kind: string): number | null {
  const key = kind.toLowerCase();
  if (key.includes("month")) return MONTH;
  if (key.includes("week")) return WEEK;
  if (key.includes("daily") || key.includes("day")) return DAY;
  if (key.includes("5h") || key.includes("five_hour") || key.includes("session")) return FIVE_HOURS;
  return null;
}

export type QuotaPace = {
  /** 窗口已过百分比，0–100。 */
  elapsedPercent: number;
  /** 已用 − 已过。正数表示烧得比时间快。 */
  delta: number;
  tone: "ahead" | "on-track" | "behind";
  label: string;
};

/** 差值在 ±5 个百分点内算持平——再窄就会因为刷新抖动来回跳。 */
const ON_TRACK = 5;

export function quotaPace(
  kind: string,
  usedPercent: number | null,
  resetsAt: string | null,
  /** 基准时刻（毫秒）。用抓取时刻而不是当下，才和百分比对得上。 */
  at: number,
): QuotaPace | null {
  if (usedPercent == null || !resetsAt || Number.isNaN(at)) return null;
  const total = windowMinutes(kind);
  if (total == null) return null;

  const reset = Date.parse(resetsAt);
  if (Number.isNaN(reset)) return null;

  const remaining = (reset - at) / MINUTE;
  // 窗口已经翻篇或时间对不上时不给结论：重置时间偶尔会滞后于真实窗口。
  if (remaining <= 0 || remaining > total) return null;

  const elapsedPercent = ((total - remaining) / total) * 100;
  const delta = usedPercent - elapsedPercent;
  const tone = delta > ON_TRACK ? "behind" : delta < -ON_TRACK ? "ahead" : "on-track";
  return {
    elapsedPercent,
    delta,
    tone,
    label:
      tone === "on-track"
        ? "节奏持平"
        : tone === "behind"
          ? `超前 ${Math.round(delta)}%`
          : `富余 ${Math.round(-delta)}%`,
  };
}
