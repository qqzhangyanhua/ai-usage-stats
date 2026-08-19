import { useCallback, useState } from "react";
import {
  readOverviewLayout,
  writeOverviewLayout,
  type OverviewLayout,
} from "../lib/overviewLayout";

export function useOverviewLayout(): {
  layout: OverviewLayout;
  setLayout: (layout: OverviewLayout) => void;
} {
  const [layout, setLayoutState] = useState<OverviewLayout>(readOverviewLayout);
  const setLayout = useCallback((next: OverviewLayout) => {
    writeOverviewLayout(next);
    setLayoutState(next);
  }, []);
  return { layout, setLayout };
}
