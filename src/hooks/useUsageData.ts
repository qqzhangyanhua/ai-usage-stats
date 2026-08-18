import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { heatmapFilter } from "../lib/calendar";
import { clearCursorSessionDetailCache } from "../lib/cursorSessionDetailCache";
import { humanStatus, previousFilter, rangeFromPreset } from "../lib/format";
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
  OfficialQuotaDto,
  OverviewDto,
  PriceTable,
  SeriesPoint,
  SessionRow,
  SourceDiagnostic,
  View,
} from "../types";
import {
  initialViewScopes,
  isViewFresh,
  reconcileLoadedStamps,
  scopesEqual,
  viewFromHash,
  viewStamp,
  viewsWarmedBy,
  type ViewScope,
} from "./viewCache";
import { emptyFilter } from "./usage/constants";
import { useAutoRefresh } from "./usage/useAutoRefresh";
import { useIngestOperations } from "./usage/useIngestOperations";
import { useSessionTurns } from "./usage/useSessionTurns";

export { viewFromHash, views } from "./viewCache";

export function useUsageData() {
  const didMount = useRef(false);
  const requestGeneration = useRef(0);
  const dataEpoch = useRef(0);
  const loadedStamps = useRef<Partial<Record<View, string>>>({});
  const optionsEpoch = useRef(-1);

  const [view, setView] = useState<View>(viewFromHash);
  const [viewScopes, setViewScopes] = useState<Record<View, ViewScope>>(initialViewScopes);
  const { filter, preset } = viewScopes[view];
  const sessionsFilter = viewScopes.sessions.filter;
  const [options, setOptions] = useState<FilterOptions>({
    sources: [],
    models: [],
    projects: [],
    providers: [],
  });
  const [overview, setOverview] = useState<OverviewDto | null>(null);
  const [billingWindows, setBillingWindows] = useState<BillingWindowsDto | null>(null);
  const [officialQuota, setOfficialQuota] = useState<OfficialQuotaDto | null>(null);
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
    sessionsFilter,
    reportError,
  );

  const markHydrated = useCallback(
    (target: View, nextFilter: Filter, nextPreset: string, scopes: Record<View, ViewScope>) => {
      const epoch = dataEpoch.current;
      const used: ViewScope = { filter: nextFilter, preset: nextPreset };
      loadedStamps.current = reconcileLoadedStamps(
        loadedStamps.current,
        target,
        used,
        scopes,
        grain,
        epoch,
      );
      setHydratedViews((current) => {
        const next = new Set(current);
        next.add(target);
        for (const warmed of viewsWarmedBy(target)) {
          const scope = warmed === target ? used : scopes[warmed];
          if (scopesEqual(scope, used)) {
            next.add(warmed);
          }
        }
        return next;
      });
    },
    [grain],
  );

  const refreshViews = useCallback(
    async (nextFilter = filter, nextPreset = preset) => {
      const generation = ++requestGeneration.current;
      const localOnly =
        view === "sessions" || view === "cursor" || view === "cursor-sessions" || view === "settings";
      if (!localOnly && !hydratedViews.has(view)) {
        setLoading(true);
      }
      const commit =
        <T>(setter: (value: T) => void) =>
        (value: T) => {
          if (generation === requestGeneration.current) {
            setter(value);
          }
        };
      const epoch = dataEpoch.current;
      const overviewFresh = isViewFresh(
        loadedStamps.current,
        "overview",
        nextFilter,
        nextPreset,
        grain,
        epoch,
      );
      const paint: Array<Promise<void>> = [];
      if (optionsEpoch.current !== epoch) {
        paint.push(
          invoke<FilterOptions>("get_filter_options").then((value) => {
            commit(setOptions)(value);
            if (generation === requestGeneration.current) {
              optionsEpoch.current = epoch;
            }
          }),
        );
      }
      if (view !== "sessions" && !overviewFresh) {
        paint.push(
          invoke<OverviewDto>("get_overview", { filter: nextFilter }).then(commit(setOverview)),
        );
      }
      const tasks: Array<Promise<void>> = [];
      if (view === "overview" || view === "trend") {
        tasks.push(
          invoke<SeriesPoint[]>("get_trend", { filter: nextFilter, grain }).then(commit(setTrend)),
        );
      }
      if (view === "overview") {
        const prev = previousFilter(nextFilter, nextPreset);
        const heat = heatmapFilter(nextFilter);
        tasks.push(
          invoke<NamedAmount[]>("get_breakdown", {
            query: { filter: nextFilter, dimension: "model" },
          }).then(commit(setModels)),
          invoke<NamedAmount[]>("get_breakdown", {
            query: { filter: nextFilter, dimension: "project" },
          }).then(commit(setProjects)),
          invoke<SessionRow[]>("get_top_sessions", { filter: nextFilter, limit: 8 }).then(
            commit(setSessions),
          ),
          invoke<BillingWindowsDto>("get_billing_windows", { filter: nextFilter }).then(
            commit(setBillingWindows),
          ),
          invoke<OfficialQuotaDto>("refresh_official_quota").then(commit(setOfficialQuota)),
          invoke<BudgetStatusDto>("get_budget_status").then(commit(setBudgetStatus)),
          invoke<SeriesPoint[]>("get_trend", { filter: heat.filter, grain: "day" }).then(
            (points) => {
              commit(setHeatmap)(points);
              commit(setHeatmapRange)({ from: heat.fromDate, to: heat.toDate });
            },
          ),
        );
        if (prev) {
          tasks.push(
            invoke<OverviewDto>("get_overview", { filter: prev }).then(commit(setPrevious)),
          );
        } else if (generation === requestGeneration.current) {
          setPrevious(null);
        }
      }
      if (view === "application") {
        tasks.push(
          invoke<ApplicationAnalyticsDto>("get_application_analytics", {
            filter: nextFilter,
            grain,
          }).then(commit(setApplicationAnalytics)),
        );
      }
      if (view === "model") {
        tasks.push(
          invoke<NamedAmount[]>("get_breakdown", {
            query: { filter: nextFilter, dimension: "model" },
          }).then(commit(setModels)),
        );
      }
      if (view === "provider") {
        tasks.push(
          invoke<NamedAmount[]>("get_breakdown", {
            query: { filter: nextFilter, dimension: "provider" },
          }).then(commit(setProviderBreakdown)),
        );
      }
      if (view === "project") {
        tasks.push(
          invoke<NamedAmount[]>("get_breakdown", {
            query: { filter: nextFilter, dimension: "project" },
          }).then(commit(setProjects)),
        );
      }
      if (view === "sessions" && selectedSession) {
        tasks.push(loadSessionTurns(selectedSession, nextFilter));
      }
      if (view === "cursor") {
        setCodeVolumeLoading(true);
        tasks.push(
          invoke<CodeVolumeSummary>("get_code_volume")
            .then(commit(setCodeVolume))
            .finally(() => {
              if (generation === requestGeneration.current) {
                setCodeVolumeLoading(false);
              }
            }),
        );
      }
      if (view === "cursor-sessions") {
        setCursorSessionLoading(true);
        tasks.push(
          invoke<CursorSessionSummaryDto>("get_cursor_session_summary")
            .then(commit(setCursorSessionSummary))
            .finally(() => {
              if (generation === requestGeneration.current) {
                setCursorSessionLoading(false);
              }
            }),
        );
      }
      if (view === "settings") {
        tasks.push(
          invoke<PriceTable>("get_prices").then(commit(setPrices)),
          invoke<SourceDiagnostic[]>("get_source_diagnostics").then(commit(setDiagnostics)),
          invoke<BudgetStatusDto>("get_budget_status").then(commit(setBudgetStatus)),
          invoke<OfficialQuotaDto>("get_official_quota").then(commit(setOfficialQuota)),
        );
      }
      try {
        await Promise.all(paint);
        if (
          generation === requestGeneration.current &&
          (view === "overview" || view === "cursor" || view === "cursor-sessions")
        ) {
          setLoading(false);
        }
        if (tasks.length > 0) {
          await Promise.all(tasks);
        }
        if (generation === requestGeneration.current) {
          markHydrated(view, nextFilter, nextPreset, viewScopes);
        }
      } finally {
        if (generation === requestGeneration.current) {
          setLoading(false);
        }
      }
      if (generation === requestGeneration.current) {
        setUpdatedAt(new Date().toISOString());
      }
    },
    [filter, preset, view, viewScopes, grain, selectedSession, loadSessionTurns, hydratedViews, markHydrated],
  );

  const refreshTrend = useCallback(
    async (nextFilter = filter) => {
      const generation = ++requestGeneration.current;
      const commit =
        <T>(setter: (value: T) => void) =>
        (value: T) => {
          if (generation === requestGeneration.current) {
            setter(value);
          }
        };
      const tasks: Array<Promise<void>> = [];
      if (view === "overview" || view === "trend") {
        tasks.push(
          invoke<SeriesPoint[]>("get_trend", { filter: nextFilter, grain }).then(commit(setTrend)),
        );
      }
      if (view === "application") {
        tasks.push(
          invoke<ApplicationAnalyticsDto>("get_application_analytics", {
            filter: nextFilter,
            grain,
          }).then(commit(setApplicationAnalytics)),
        );
      }
      if (tasks.length === 0) {
        return;
      }
      setLoading(true);
      try {
        await Promise.all(tasks);
      } finally {
        if (generation === requestGeneration.current) {
          setLoading(false);
          if (view === "overview" || view === "trend") {
            markHydrated(view, nextFilter, preset, viewScopes);
            if (
              view === "trend" &&
              scopesEqual(viewScopes.overview, { filter: nextFilter, preset })
            ) {
              loadedStamps.current.overview = viewStamp(
                "overview",
                nextFilter,
                preset,
                grain,
                dataEpoch.current,
              );
            }
          }
          if (view === "application") {
            markHydrated("application", nextFilter, preset, viewScopes);
          }
        }
      }
      if (generation === requestGeneration.current) {
        setUpdatedAt(new Date().toISOString());
      }
    },
    [filter, view, viewScopes, grain, preset, markHydrated],
  );

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
      const range = explicitRange ?? rangeFromPreset(next);
      const nextFilter = { ...filter, ...range };
      setViewScopes((current) => ({
        ...current,
        [view]: { filter: nextFilter, preset: next },
      }));
      refreshViews(nextFilter, next).catch(reportError);
    },
    [filter, view, refreshViews, reportError],
  );

  const applyViewFilter = useCallback(
    (target: View, next: Filter) => {
      setViewScopes((current) => ({
        ...current,
        [target]: { filter: next, preset: current[target].preset },
      }));
      if (target === view) {
        refreshViews(next).catch(reportError);
      }
    },
    [view, refreshViews, reportError],
  );

  const applyFilter = useCallback(
    (next: Filter) => {
      applyViewFilter(view, next);
    },
    [applyViewFilter, view],
  );

  const applySessionsFilter = useCallback(
    (next: Filter) => {
      applyViewFilter("sessions", next);
    },
    [applyViewFilter],
  );

  return {
    view,
    filter,
    sessionsFilter,
    preset,
    options,
    overview,
    billingWindows,
    officialQuota,
    setOfficialQuota,
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
    applySessionsFilter,
    openSessions,
    runIngest: runIngestWithCacheClear,
    runRebuild: runRebuildWithCacheClear,
    runPurgeArchived,
    reportError,
  };
}
