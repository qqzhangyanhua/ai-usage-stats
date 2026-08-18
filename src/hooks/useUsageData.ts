import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { heatmapFilter } from "../lib/calendar";
import { clearCursorSessionDetailCache } from "../lib/cursorSessionDetailCache";
import { humanStatus, rangeFromPreset } from "../lib/format";
import type {
  ApplicationAnalyticsDto,
  BillingWindowsDto,
  BudgetConfig,
  BudgetStatusDto,
  CodeVolumeSummary,
  CursorSessionSummaryDto,
  Filter,
  FilterOptions,
  Grain,
  IngestReport,
  NamedAmount,
  OverviewDto,
  PriceTable,
  SeriesPoint,
  SessionRow,
  SourceDiagnostic,
  View,
} from "../types";
import { isViewFresh, viewFromHash, viewsWarmedBy, viewStamp } from "./viewCache";
import { emptyFilter } from "./usage/constants";
import { useAutoRefresh } from "./usage/useAutoRefresh";
import { useIngestOperations } from "./usage/useIngestOperations";
import { useSessionTurns } from "./usage/useSessionTurns";
import { useViewRefresh } from "./usage/useViewRefresh";

export { viewFromHash, views } from "./viewCache";

export function useUsageData() {
  const didMount = useRef(false);
  const requestGeneration = useRef(0);
  const dataEpoch = useRef(0);
  const loadedStamps = useRef<Partial<Record<View, string>>>({});
  const optionsEpoch = useRef(-1);

  const [view, setView] = useState<View>(viewFromHash);
  const [filter, setFilter] = useState<Filter>(emptyFilter);
  const [preset, setPreset] = useState("all");
  const [options, setOptions] = useState<FilterOptions>({
    sources: [],
    models: [],
    projects: [],
    providers: [],
  });
  const [overview, setOverview] = useState<OverviewDto | null>(null);
  const [billingWindows, setBillingWindows] = useState<BillingWindowsDto | null>(null);
  const [previous, setPrevious] = useState<OverviewDto | null>(null);
  const [trend, setTrend] = useState<SeriesPoint[]>([]);
  const [heatmap, setHeatmap] = useState<SeriesPoint[]>([]);
  const [heatmapRange, setHeatmapRange] = useState(() => {
    const window = heatmapFilter(emptyFilter);
    return { from: window.fromDate, to: window.toDate };
  });
  const [grain, setGrain] = useState<Grain>("day");
  const [applicationAnalytics, setApplicationAnalytics] = useState<ApplicationAnalyticsDto | null>(
    null,
  );
  const [models, setModels] = useState<NamedAmount[]>([]);
  const [projects, setProjects] = useState<NamedAmount[]>([]);
  const [providerBreakdown, setProviderBreakdown] = useState<NamedAmount[]>([]);
  const [hydratedViews, setHydratedViews] = useState<Set<View>>(() => new Set());
  const [sessions, setSessions] = useState<SessionRow[]>([]);
  const [sessionsRevision, setSessionsRevision] = useState(0);
  const [sessionsVisited, setSessionsVisited] = useState(() => viewFromHash() === "sessions");
  const [prices, setPrices] = useState<PriceTable>({ prices: [] });
  const [budgetStatus, setBudgetStatus] = useState<BudgetStatusDto | null>(null);
  const [savingBudget, setSavingBudget] = useState(false);
  const [diagnostics, setDiagnostics] = useState<SourceDiagnostic[]>([]);
  const [lastIngestReport, setLastIngestReport] = useState<IngestReport | null>(null);
  const [codeVolume, setCodeVolume] = useState<CodeVolumeSummary | null>(null);
  const [codeVolumeLoading, setCodeVolumeLoading] = useState(() => viewFromHash() === "cursor");
  const [cursorSessionSummary, setCursorSessionSummary] = useState<CursorSessionSummaryDto | null>(
    null,
  );
  const [cursorSessionLoading, setCursorSessionLoading] = useState(
    () => viewFromHash() === "cursor-sessions",
  );
  const [status, setStatus] = useState("正在连接…");
  const [connected, setConnected] = useState(false);
  const [loading, setLoading] = useState(true);
  const [updatedAt, setUpdatedAt] = useState<string | null>(null);

  const reportError = useCallback((error: unknown) => {
    setStatus(humanStatus(error));
  }, []);

  const { turns, turnsLoading, selectedSession, loadSessionTurns, selectSession } = useSessionTurns(
    filter,
    reportError,
  );

  const markHydrated = useCallback(
    (target: View, nextFilter: Filter, nextPreset: string) => {
      const epoch = dataEpoch.current;
      for (const warmed of viewsWarmedBy(target)) {
        loadedStamps.current[warmed] = viewStamp(warmed, nextFilter, nextPreset, grain, epoch);
      }
      setHydratedViews((current) => {
        const next = new Set(current);
        for (const warmed of viewsWarmedBy(target)) {
          next.add(warmed);
        }
        return next;
      });
    },
    [grain],
  );

  const { refreshViews, refreshTrend } = useViewRefresh({
    view,
    filter,
    preset,
    grain,
    selectedSession,
    hydratedViews,
    loadSessionTurns,
    requestGenerationRef: requestGeneration,
    dataEpochRef: dataEpoch,
    loadedStampsRef: loadedStamps,
    optionsEpochRef: optionsEpoch,
    markHydrated,
    setLoading,
    setOptions,
    setOverview,
    setTrend,
    setModels,
    setProjects,
    setSessions,
    setBillingWindows,
    setBudgetStatus,
    setHeatmap,
    setHeatmapRange,
    setPrevious,
    setApplicationAnalytics,
    setProviderBreakdown,
    setCodeVolume,
    setCodeVolumeLoading,
    setCursorSessionSummary,
    setCursorSessionLoading,
    setPrices,
    setDiagnostics,
    setUpdatedAt,
  });

  const wrappedRefreshViews = useCallback(async () => {
    await refreshViews();
  }, [refreshViews]);

  const { busy, rebuilding, purging, runIngest, runRebuild, runPurgeArchived } =
    useIngestOperations({
      refreshViews: wrappedRefreshViews,
      dataEpochRef: dataEpoch,
      requestGenerationRef: requestGeneration,
      setSessionsRevision,
      setLastIngestReport,
      setStatus,
      setLoading,
    });

  const runIngestWithCacheClear = useCallback(
    async (label: string) => {
      clearCursorSessionDetailCache();
      await runIngest(label);
    },
    [runIngest],
  );

  const runRebuildWithCacheClear = useCallback(
    async (source: string | null) => {
      clearCursorSessionDetailCache();
      await runRebuild(source);
    },
    [runRebuild],
  );

  const { autoRefresh, setAutoRefresh } = useAutoRefresh(runIngestWithCacheClear, reportError);

  const saveBudget = useCallback(
    async (config: BudgetConfig) => {
      setSavingBudget(true);
      try {
        await invoke("save_budget", { config });
        const nextStatus = await invoke<BudgetStatusDto>("get_budget_status");
        setBudgetStatus(nextStatus);
        setStatus("预算设置已保存");
      } catch (error) {
        setStatus(`预算设置保存失败：${humanStatus(error)}`);
        throw error;
      } finally {
        setSavingBudget(false);
      }
    },
    [],
  );

  const runIngestRef = useRef(runIngestWithCacheClear);
  useEffect(() => {
    runIngestRef.current = runIngestWithCacheClear;
  }, [runIngestWithCacheClear]);

  useEffect(() => {
    invoke<string>("ping")
      .then(async () => {
        setConnected(true);
        setStatus("正在加载缓存…");
        try {
          await refreshViews();
        } catch (error: unknown) {
          reportError(error);
          setLoading(false);
        }
        return runIngestRef.current("启动摄取");
      })
      .catch((error: unknown) => {
        setConnected(false);
        setStatus(humanStatus(error));
        setLoading(false);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 只在启动时拉一次缓存并后台摄取
  }, []);

  useEffect(() => {
    if (!didMount.current) {
      didMount.current = true;
      return;
    }
    if (view === "sessions") {
      return;
    }
    if (isViewFresh(loadedStamps.current, view, filter, preset, grain, dataEpoch.current)) {
      return;
    }
    refreshViews().catch(reportError);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 热缓存命中则不重拉；会话页自己管列表
  }, [view]);

  const didMountGrain = useRef(false);
  useEffect(() => {
    if (!didMountGrain.current) {
      didMountGrain.current = true;
      return;
    }
    refreshTrend().catch(reportError);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 只需在 grain 变化时触发
  }, [grain]);

  const openSessions = useCallback(() => {
    setSessionsVisited(true);
    setView("sessions");
    window.history.replaceState(null, "", "#sessions");
  }, []);

  const navigate = useCallback((next: View) => {
    if (next === "sessions") {
      setSessionsVisited(true);
    }
    setView(next);
    if (window.location.hash.replace(/^#/, "") !== next) {
      window.history.replaceState(null, "", `#${next}`);
    }
  }, []);

  const applyPreset = useCallback(
    (next: string, explicitRange?: { from: string | null; to: string | null }) => {
      setPreset(next);
      const range = explicitRange ?? rangeFromPreset(next);
      const nextFilter = { ...filter, ...range };
      setFilter(nextFilter);
      refreshViews(nextFilter, next).catch(reportError);
    },
    [filter, refreshViews, reportError],
  );

  const applyFilter = useCallback(
    (next: Filter) => {
      setFilter(next);
      refreshViews(next).catch(reportError);
    },
    [refreshViews, reportError],
  );

  return {
    view,
    filter,
    preset,
    options,
    overview,
    billingWindows,
    previous,
    trend,
    heatmap,
    heatmapRange,
    grain,
    setGrain,
    breakdown:
      view === "provider" ? providerBreakdown : view === "project" ? projects : models,
    applicationAnalytics,
    models,
    projects,
    sessions,
    sessionsRevision,
    sessionsVisited,
    turns,
    turnsLoading,
    selectedSession,
    setSelectedSession: selectSession,
    prices,
    setPrices,
    budgetStatus,
    savingBudget,
    saveBudget,
    diagnostics,
    lastIngestReport,
    rebuilding,
    purging,
    codeVolume,
    codeVolumeLoading,
    cursorSessionSummary,
    cursorSessionLoading,
    status,
    setStatus,
    connected,
    busy,
    loading,
    viewHasData: hydratedViews.has(view),
    updatedAt,
    autoRefresh,
    setAutoRefresh,
    navigate,
    applyPreset,
    applyFilter,
    openSessions,
    runIngest: runIngestWithCacheClear,
    runRebuild: runRebuildWithCacheClear,
    runPurgeArchived,
    reportError,
  };
}
