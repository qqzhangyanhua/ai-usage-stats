import { describe, expect, it } from "vitest";
import {
  popRangeHistory,
  pushRangeHistory,
  rangeSnapshot,
  sameRange,
} from "./rangeHistory";

const month = rangeSnapshot("30", "2026-08-01T00:00:00.000Z", "2026-08-30T23:59:59.999Z");
const week = rangeSnapshot("custom", "2026-08-17T00:00:00.000Z", "2026-08-23T23:59:59.999Z");
const day = rangeSnapshot("custom", "2026-08-19T00:00:00.000Z", "2026-08-19T23:59:59.999Z");

describe("sameRange", () => {
  it("compares preset and the exact from/to window", () => {
    expect(sameRange(month, month)).toBe(true);
    expect(sameRange(month, { ...month, preset: "custom" })).toBe(false);
    expect(sameRange(week, day)).toBe(false);
  });
});

describe("pushRangeHistory", () => {
  it("keeps the same array when the next window is identical", () => {
    const history: typeof month[] = [];
    expect(pushRangeHistory(history, day, day)).toBe(history);
  });

  it("appends the current window so later pops can restore it", () => {
    expect(pushRangeHistory([], month, week)).toEqual([month]);
    expect(pushRangeHistory([month], week, day)).toEqual([month, week]);
  });
});

describe("popRangeHistory", () => {
  it("returns null on an empty stack", () => {
    expect(popRangeHistory([])).toEqual({ history: [], previous: null });
  });

  it("restores the last pushed window and leaves earlier levels", () => {
    expect(popRangeHistory([month, week])).toEqual({ history: [month], previous: week });
    expect(popRangeHistory([month])).toEqual({ history: [], previous: month });
  });
});
