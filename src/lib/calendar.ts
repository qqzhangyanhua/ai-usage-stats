const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"] as const;

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
