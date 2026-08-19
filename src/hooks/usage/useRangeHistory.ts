import { useCallback, useMemo, useState } from "react";
import {
  popRangeHistory,
  pushRangeHistory,
  sameRange,
  type RangeSnapshot,
} from "../../lib/rangeHistory";
import type { View } from "../../types";

export function useRangeHistory(view: View) {
  const [histories, setHistories] = useState<Partial<Record<View, RangeSnapshot[]>>>({});

  const canGoBack = (histories[view] ?? []).length > 0;

  const pushCurrent = useCallback(
    (current: RangeSnapshot, next: RangeSnapshot): boolean => {
      if (sameRange(current, next)) {
        return false;
      }
      setHistories((hist) => ({
        ...hist,
        [view]: pushRangeHistory(hist[view] ?? [], current, next),
      }));
      return true;
    },
    [view],
  );

  const pop = useCallback((): RangeSnapshot | null => {
    const popped = popRangeHistory(histories[view] ?? []);
    if (!popped.previous) {
      return null;
    }
    setHistories((hist) => {
      const latest = popRangeHistory(hist[view] ?? []);
      return latest.previous ? { ...hist, [view]: latest.history } : hist;
    });
    return popped.previous;
  }, [histories, view]);

  const clear = useCallback(() => {
    setHistories((hist) => (hist[view]?.length ? { ...hist, [view]: [] } : hist));
  }, [view]);

  return useMemo(
    () => ({ canGoBack, pushCurrent, pop, clear }),
    [canGoBack, clear, pop, pushCurrent],
  );
}
