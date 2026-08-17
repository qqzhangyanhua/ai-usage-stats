import type { Filter } from "../types";

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"] as const;
const HEATMAP_WEEKS = 53;

export function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

export function toDateValue(date: Date): string {
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`;
}

export function parseDateValue(value: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) {
    return null;
  }
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(year, month - 1, day);
  if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) {
    return null;
  }
  return date;
}

export function formatDateLabel(value: string): string {
  const date = parseDateValue(value);
  if (!date) {
    return "选择日期";
  }
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`;
}

export function monthTitle(year: number, month: number): string {
  return `${year} 年 ${month + 1} 月`;
}

export function weekdayLabels(): readonly string[] {
  return WEEKDAYS;
}

/** 生成当月日历格子，周一为一周起点；前后月补齐。 */
export function calendarCells(year: number, month: number): { value: string; inMonth: boolean }[] {
  const first = new Date(year, month, 1);
  const startOffset = (first.getDay() + 6) % 7;
  const start = new Date(year, month, 1 - startOffset);
  return Array.from({ length: 42 }, (_, index) => {
    const date = new Date(start);
    date.setDate(start.getDate() + index);
    return {
      value: toDateValue(date),
      inMonth: date.getMonth() === month,
    };
  });
}

export function shiftMonth(
  year: number,
  month: number,
  delta: number,
): { year: number; month: number } {
  const next = new Date(year, month + delta, 1);
  return { year: next.getFullYear(), month: next.getMonth() };
}

export type HeatmapWindow = {
  from: string;
  to: string;
  fromDate: string;
  toDate: string;
};

/** 以 end 所在周为最后一列，往前取 53 周（周一起始），返回查询用 ISO 与日历用日期。 */
export function heatmapWindow(end: Date): HeatmapWindow {
  const safeEnd = Number.isNaN(end.getTime()) ? new Date() : end;
  const endDay = new Date(safeEnd.getFullYear(), safeEnd.getMonth(), safeEnd.getDate());
  const mondayOffset = (endDay.getDay() + 6) % 7;
  const thisMonday = new Date(endDay);
  thisMonday.setDate(endDay.getDate() - mondayOffset);
  const startMonday = new Date(thisMonday);
  startMonday.setDate(thisMonday.getDate() - (HEATMAP_WEEKS - 1) * 7);
  const fromDate = toDateValue(startMonday);
  const toDate = toDateValue(endDay);
  return {
    from: new Date(`${fromDate}T00:00:00`).toISOString(),
    to: new Date(`${toDate}T23:59:59.999`).toISOString(),
    fromDate,
    toDate,
  };
}

/** 保留来源/模型/项目筛选，日期窗口固定为近 53 周。 */
export function heatmapFilter(filter: Filter): {
  filter: Filter;
  fromDate: string;
  toDate: string;
} {
  const parsed = filter.to ? new Date(filter.to) : new Date();
  const window = heatmapWindow(parsed);
  return {
    filter: {
      ...filter,
      from: window.from,
      to: window.to,
    },
    fromDate: window.fromDate,
    toDate: window.toDate,
  };
}
