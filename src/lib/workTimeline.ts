import type { WorkSegment } from "../types";

export const DAY_MINUTES = 24 * 60;
/** 极短/零时长的会话仍给这么多分钟宽度，保证在时间轴上可见、可点击。 */
export const MIN_SEGMENT_MINUTES = 6;

export type LaneSegment = {
  segment: WorkSegment;
  startMinutes: number;
  endMinutes: number;
  lane: number;
};

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** 把一个 ISO 时刻换算成距 `dayStartIso` 的分钟数，钳制在 [0, 1440]。两者都是完整 ISO
 * 时刻（带偏移），用 epoch 差值计算，不依赖运行环境的时区设置。 */
export function minutesSinceDayStart(iso: string, dayStartIso: string): number {
  const start = Date.parse(dayStartIso);
  const at = Date.parse(iso);
  if (Number.isNaN(start) || Number.isNaN(at)) {
    return 0;
  }
  return clamp(Math.round((at - start) / 60_000), 0, DAY_MINUTES);
}

/**
 * 泳道贪心布局：按开始时间排序后，把每个片段放进第一条「结束时间不晚于当前片段开始时间」
 * 的已有泳道；找不到就新开一条。零/极短时长的片段会被撑到 `MIN_SEGMENT_MINUTES` 宽，
 * 但撑宽只影响布局，不改变片段本身携带的时间/token 数据。
 */
export function layoutSegments(segments: WorkSegment[], dayStartIso: string): LaneSegment[] {
  const withMinutes = segments
    .map((segment) => {
      const startMinutes = minutesSinceDayStart(segment.start, dayStartIso);
      const rawEnd = minutesSinceDayStart(segment.end, dayStartIso);
      const endMinutes = Math.min(
        DAY_MINUTES,
        Math.max(rawEnd, startMinutes + MIN_SEGMENT_MINUTES),
      );
      return { segment, startMinutes, endMinutes };
    })
    .sort((a, b) => a.startMinutes - b.startMinutes || a.endMinutes - b.endMinutes);

  const laneEnds: number[] = [];
  const placed: LaneSegment[] = [];
  for (const item of withMinutes) {
    let lane = laneEnds.findIndex((end) => end <= item.startMinutes);
    if (lane === -1) {
      lane = laneEnds.length;
      laneEnds.push(item.endMinutes);
    } else {
      laneEnds[lane] = item.endMinutes;
    }
    placed.push({ ...item, lane });
  }
  return placed;
}

/** 布局用掉的泳道数，即时间轴上出现过的「同时进行」峰值。 */
export function laneCount(layout: LaneSegment[]): number {
  return layout.reduce((max, item) => Math.max(max, item.lane + 1), 0);
}

/** 所选本地日历日（YYYY-MM-DD）零点对应的 ISO 时刻，供 `layoutSegments` 当基准。 */
export function dayStartIso(day: string): string {
  return new Date(`${day}T00:00:00`).toISOString();
}
