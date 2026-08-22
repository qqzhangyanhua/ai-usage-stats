import { describe, expect, it } from "vitest";
import { quotaPace, windowMinutes } from "./quotaPace";

const NOW = Date.parse("2026-08-22T12:00:00Z");
const minutesFromNow = (minutes: number) => new Date(NOW + minutes * 60_000).toISOString();

describe("windowMinutes", () => {
  it("认得各家 kind 里写着的周期", () => {
    // 各 provider 的实际 kind，不是编的。
    expect(windowMinutes("session_5h")).toBe(300);
    expect(windowMinutes("gemini_5h")).toBe(300);
    expect(windowMinutes("weekly")).toBe(10080);
    expect(windowMinutes("weekly_sonnet")).toBe(10080);
    expect(windowMinutes("core_weekly")).toBe(10080);
    expect(windowMinutes("monthly")).toBe(43200);
    expect(windowMinutes("daily")).toBe(1440);
  });

  it("先匹配长周期，避免 weekly 被 day 抢走", () => {
    expect(windowMinutes("weekly")).toBe(10080);
    expect(windowMinutes("monthly")).toBe(43200);
  });

  it("推不出来的返回 null 而不是猜一个", () => {
    // Cursor 的计费周期长度随账单日变化，OpenCode 的 rolling 也没写周期。
    expect(windowMinutes("billing_cycle")).toBeNull();
    expect(windowMinutes("auto")).toBeNull();
    expect(windowMinutes("on_demand")).toBeNull();
    expect(windowMinutes("credits")).toBeNull();
  });
});

describe("quotaPace", () => {
  it("窗口过半时用量也过半算持平", () => {
    const pace = quotaPace("weekly", 50, minutesFromNow(10080 / 2), NOW);
    expect(pace?.elapsedPercent).toBeCloseTo(50);
    expect(pace?.tone).toBe("on-track");
    expect(pace?.label).toBe("节奏持平");
  });

  it("烧得比时间快时报超前", () => {
    // 5 小时窗刚过 1/5，却已经用了 80%。
    const pace = quotaPace("session_5h", 80, minutesFromNow(240), NOW);
    expect(pace?.tone).toBe("behind");
    expect(pace?.delta).toBeCloseTo(60);
    expect(pace?.label).toBe("超前 60%");
  });

  it("用得比时间慢时报富余", () => {
    const pace = quotaPace("weekly", 10, minutesFromNow(10080 * 0.25), NOW);
    expect(pace?.tone).toBe("ahead");
    expect(pace?.label).toBe("富余 65%");
  });

  it("缺数据或窗口对不上时不给结论", () => {
    expect(quotaPace("weekly", null, minutesFromNow(100), NOW)).toBeNull();
    expect(quotaPace("weekly", 50, null, NOW)).toBeNull();
    expect(quotaPace("billing_cycle", 50, minutesFromNow(100), NOW)).toBeNull();
    expect(quotaPace("weekly", 50, "not a date", NOW)).toBeNull();
    // 已经过了重置时间：窗口翻篇了，旧百分比算不出节奏。
    expect(quotaPace("weekly", 50, minutesFromNow(-1), NOW)).toBeNull();
    // 剩余比窗口总长还多，说明重置时间对不上，别硬算。
    expect(quotaPace("session_5h", 50, minutesFromNow(600), NOW)).toBeNull();
  });

  it("±5 个百分点内算持平，避免刷新时来回跳", () => {
    expect(quotaPace("weekly", 54, minutesFromNow(10080 / 2), NOW)?.tone).toBe("on-track");
    expect(quotaPace("weekly", 46, minutesFromNow(10080 / 2), NOW)?.tone).toBe("on-track");
    expect(quotaPace("weekly", 60, minutesFromNow(10080 / 2), NOW)?.tone).toBe("behind");
  });
});

describe("quotaPace 基准时刻", () => {
  it("基准无效时不给结论", () => {
    // captured_at 缺失时前端传进来的是 NaN，不能算成 1970 年。
    expect(quotaPace("weekly", 50, minutesFromNow(100), Number.NaN)).toBeNull();
  });
});
