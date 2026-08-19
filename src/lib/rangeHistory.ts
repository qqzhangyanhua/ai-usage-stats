export type RangeSnapshot = {
  preset: string;
  from: string | null;
  to: string | null;
};

export function sameRange(left: RangeSnapshot, right: RangeSnapshot): boolean {
  return left.preset === right.preset && left.from === right.from && left.to === right.to;
}

export function rangeSnapshot(
  preset: string,
  from: string | null,
  to: string | null,
): RangeSnapshot {
  return { preset, from, to };
}

/** 下钻时压入当前范围；同一范围再点一次不入栈。 */
export function pushRangeHistory(
  history: RangeSnapshot[],
  current: RangeSnapshot,
  next: RangeSnapshot,
): RangeSnapshot[] {
  if (sameRange(current, next)) {
    return history;
  }
  return [...history, current];
}

export function popRangeHistory(history: RangeSnapshot[]): {
  history: RangeSnapshot[];
  previous: RangeSnapshot | null;
} {
  const previous = history[history.length - 1];
  if (!previous) {
    return { history, previous: null };
  }
  return { history: history.slice(0, -1), previous };
}
