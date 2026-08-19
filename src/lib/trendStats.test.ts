import { describe, expect, it } from "vitest";
import type { SeriesPoint } from "../types";
import { summarizeTrend, trendTableRows, trendTableRowsNewestFirst } from "./trendStats";

function point(
  bucket: string,
  total: number,
  extras: Partial<Pick<SeriesPoint, "input_tokens" | "output_tokens" | "cost">> = {},
): SeriesPoint {
  return {
    bucket,
    total_tokens: total,
    input_tokens: extras.input_tokens ?? total,
    output_tokens: extras.output_tokens ?? 0,
    cost: extras.cost ?? null,
  };
}

describe("summarizeTrend", () => {
  it("returns empty defaults when there are no points", () => {
    expect(summarizeTrend([])).toEqual({
      totalTokens: 0,
      hasCost: false,
      totalCost: 0,
      bucketAvg: 0,
      peak: null,
      sparkTokens: [],
      sparkCost: [],
      maxTotal: 1,
    });
  });

  it("aggregates tokens, cost sparks, and the peak bucket in one pass", () => {
    const points = [
      point("2026-08-01", 100, { cost: 1 }),
      point("2026-08-02", 300, { cost: 3 }),
      point("2026-08-03", 200, { cost: null }),
    ];
    const stats = summarizeTrend(points);
    expect(stats.totalTokens).toBe(600);
    expect(stats.hasCost).toBe(true);
    expect(stats.totalCost).toBe(4);
    expect(stats.bucketAvg).toBe(200);
    expect(stats.peak?.bucket).toBe("2026-08-02");
    expect(stats.sparkTokens).toEqual([100, 300, 200]);
    expect(stats.sparkCost).toEqual([1, 3, 0]);
    expect(stats.maxTotal).toBe(300);
  });
});

describe("trendTableRows", () => {
  it("computes share vs peak/total and period delta against the previous bucket", () => {
    const rows = trendTableRows([
      point("2026-08-01", 100),
      point("2026-08-02", 200),
      point("2026-08-03", 100),
    ]);
    expect(rows[0]?.periodDelta).toBeNull();
    expect(rows[1]?.periodDelta).toBe(100);
    expect(rows[2]?.periodDelta).toBe(-50);
    expect(rows[1]?.shareOfMax).toBe(100);
    expect(rows[0]?.shareOfTotal).toBe(25);
    expect(rows[1]?.shareOfTotal).toBe(50);
  });

  it("keeps period delta when reversing to newest-first", () => {
    const rows = trendTableRowsNewestFirst([
      point("2026-08-01", 100),
      point("2026-08-02", 200),
    ]);
    expect(rows.map((row) => row.point.bucket)).toEqual(["2026-08-02", "2026-08-01"]);
    expect(rows[0]?.periodDelta).toBe(100);
    expect(rows[1]?.periodDelta).toBeNull();
  });
});
