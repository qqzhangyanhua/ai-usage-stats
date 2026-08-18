import { useEffect, useRef, useState } from "react";
import { AUTO_REFRESH_STORAGE_KEY, loadAutoRefresh } from "./constants";

export function useAutoRefresh(
  runIngest: (label: string) => Promise<void>,
  reportError: (error: unknown) => void,
) {
  const [autoRefresh, setAutoRefresh] = useState<string>(loadAutoRefresh);
  const runIngestRef = useRef(runIngest);

  useEffect(() => {
    runIngestRef.current = runIngest;
  }, [runIngest]);

  useEffect(() => {
    try {
      window.localStorage.setItem(AUTO_REFRESH_STORAGE_KEY, autoRefresh);
    } catch {
      // localStorage 不可用时忽略，仅影响下次启动是否记住选择
    }
    const minutes = Number(autoRefresh);
    if (autoRefresh === "off" || !Number.isFinite(minutes) || minutes <= 0) {
      return;
    }
    const id = window.setInterval(() => {
      runIngestRef.current("定时刷新").catch(reportError);
    }, minutes * 60_000);
    return () => window.clearInterval(id);
  }, [autoRefresh, reportError]);

  return { autoRefresh, setAutoRefresh };
}
