import type { SeriesPoint } from "../types";
import { deltaPct } from "./format";

export type TrendStats = {
  totalTokens: number;
  hasCost: boolean;
  totalCost: number;
  bucketAvg: number;
  peak: SeriesPoint | null;
  sparkTokens: number[];
  sparkCost: number[];
  maxTotal: number;
};

export type TrendTableRow = {
  point: SeriesPoint;
  chronologicalIndex: number;
  shareOfMax: number;
  shareOfTotal: number;
  periodDelta: number | null;
};

export function summarizeTrend(points: SeriesPoint[]): TrendStats {
  if (points.length === 0) {
    return {
      totalTokens: 0,
      hasCost: false,
      totalCost: 0,
      bucketAvg: 0,
      peak: null,
      sparkTokens: [],
      sparkCost: [],
      maxTotal: 1,
    };
  }

  let totalTokens = 0;
  let totalCost = 0;
  let hasCost = false;
  let peak = points[0];
  const sparkTokens: number[] = [];
  const sparkCost: number[] = [];

  for (const point of points) {
    totalTokens += point.total_tokens;
    sparkTokens.push(point.total_tokens);
    sparkCost.push(point.cost ?? 0);
    if (point.cost != null) {
      hasCost = true;
      totalCost += point.cost;
    }
    if (point.total_tokens > peak.total_tokens) {
      peak = point;
    }
  }

  return {
    totalTokens,
    hasCost,
    totalCost,
    bucketAvg: totalTokens / points.length,
    peak,
    sparkTokens,
    sparkCost,
    maxTotal: Math.max(1, peak.total_tokens),
  };
}

export function trendTableRows(points: SeriesPoint[]): TrendTableRow[] {
  const { totalTokens, maxTotal } = summarizeTrend(points);
  const denom = Math.max(totalTokens, 1);
  return points.map((point, chronologicalIndex) => {
    const previous = chronologicalIndex > 0 ? points[chronologicalIndex - 1] : undefined;
    return {
      point,
      chronologicalIndex,
      shareOfMax: (point.total_tokens / maxTotal) * 100,
      shareOfTotal: (point.total_tokens / denom) * 100,
      periodDelta: previous ? deltaPct(point.total_tokens, previous.total_tokens) : null,
    };
  });
}

/** 明细表按时间倒序，最新 bucket 在上；环比仍相对更早的那个时间桶。 */
export function trendTableRowsNewestFirst(points: SeriesPoint[]): TrendTableRow[] {
  return [...trendTableRows(points)].reverse();
}
