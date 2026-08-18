import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { heatmapFilter } from "../lib/calendar";
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
  OverviewDto,
  PriceTable,
  SeriesPoint,
  SessionRow,
  SourceDiagnostic,
  TurnRow,
  View,
} from "../types";
import { isViewFresh, viewFromHash, viewsWarmedBy, viewStamp } from "./viewCache";

export { viewFromHash, views } from "./viewCache";

const AUTO_REFRESH_STORAGE_KEY = "ai-usage-stats:auto-refresh";

function loadAutoRefresh(): string {
  try {
    return window.localStorage.getItem(AUTO_REFRESH_STORAGE_KEY) ?? "off";
  } catch {
    return "off";
  }
}

const emptyFilter: Filter = {
  from: null,
  to: null,
  sources: [],
  models: [],
  projects: [],
  providers: [],
};

type SelectedSession = { id: string; source: string };

export function useUsageData() {
  const didMount = useRef(false);
  const requestGeneration = useRef(0);
  const turnsGeneration = useRef(0);
  const ingestOperation = useRef(false);
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
  const [turns, setTurns] = useState<TurnRow[]>([]);
  const [turnsLoading, setTurnsLoading] = useState(false);
  const [selectedSession, setSelectedSession] = useState<SelectedSession | null>(null);
  const [sessionsVisited, setSessionsVisited] = useState(() => viewFromHash() === "sessions");
  const [prices, setPrices] = useState<PriceTable>({ prices: [] });
  const [budgetStatus, setBudgetStatus] = useState<BudgetStatusDto | null>(null);
  const [savingBudget, setSavingBudget] = useState(false);
  const [diagnostics, setDiagnostics] = useState<SourceDiagnostic[]>([]);
  const [lastIngestReport, setLastIngestReport] = useState<IngestReport | null>(null);
  const [rebuilding, setRebuilding] = useState<string | null>(null);
  const [purging, setPurging] = useState<string | null>(null);
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
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [updatedAt, setUpdatedAt] = useState<string | null>(null);
  const [autoRefresh, setAutoRefresh] = useState<string>(loadAutoRefresh);

  const loadSessionTurns = useCallback(
    async (session: SelectedSession, nextFilter = filter) => {
      const generation = ++turnsGeneration.current;
      setTurnsLoading(true);
      try {
        const rows = await invoke<TurnRow[]>("get_session_turns", {
          sessionId: session.id,
          source: session.source,
          filter: nextFilter,
        });
        if (generation === turnsGeneration.current) {
          setTurns(rows);
        }
      } finally {
        if (generation === turnsGeneration.current) {
          setTurnsLoading(false);
        }
      }
    },
    [filter],
  );

  const markHydrated = useCallback((target: View, nextFilter: Filter, nextPreset: string) => {
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
  }, [grain]);

  const refreshViews = useCallback(
    async (nextFilter = filter, nextPreset = preset) => {
      const generation = ++requestGeneration.current;
      // 会话 / Cursor / 已有缓存的页不要用全屏遮罩。
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
          markHydrated(view, nextFilter, nextPreset);
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
    [filter, preset, view, grain, selectedSession, loadSessionTurns, hydratedViews, markHydrated],
  );

  // 切换粒度（按日/按周/按月）时只重新拉取趋势相关数据，避免刷新整页导致的卡顿。
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
            markHydrated("trend", nextFilter, preset);
            loadedStamps.current.overview = viewStamp(
              "overview",
              nextFilter,
              preset,
              grain,
              dataEpoch.current,
            );
          }
          if (view === "application") {
            markHydrated("application", nextFilter, preset);
          }
        }
      }
      if (generation === requestGeneration.current) {
        setUpdatedAt(new Date().toISOString());
      }
    },
    [filter, view, grain, preset, markHydrated],
  );

  const reportError = useCallback((error: unknown) => {
    setStatus(humanStatus(error));
  }, []);

  const runIngest = useCallback(
    async (label: string) => {
      if (ingestOperation.current) {
        return;
      }
      ingestOperation.current = true;
      requestGeneration.current += 1;
      setBusy(true);
      setStatus(`${label}中…`);
      try {
        const report = await invoke<IngestReport>("ingest");
        dataEpoch.current += 1;
        setSessionsRevision((n) => n + 1);
        setLastIngestReport(report);
        const issue = report.files_failed > 0 ? `，失败 ${report.files_failed}` : "";
        const removed = report.records_removed > 0 ? `，清理 ${report.records_removed}` : "";
        const archived = report.records_archived > 0 ? `，归档 ${report.records_archived}` : "";
        setStatus(
          `${label}${report.partial_success ? "部分完成" : "完成"}：解析 ${report.files_parsed}，跳过 ${report.files_skipped}，写入 ${report.records_written}${archived}${removed}${issue}`,
        );
        await refreshViews();
        try {
          await invoke("refresh_tray");
        } catch {
          // 菜单栏刷新失败不阻断主界面
        }
      } catch (error) {
        setStatus(`${label}失败：${humanStatus(error)}`);
        setLoading(false);
      } finally {
        ingestOperation.current = false;
        setBusy(false);
      }
    },
    [refreshViews],
  );

  const runRebuild = useCallback(
    async (source: string | null) => {
      if (ingestOperation.current) {
        return;
      }
      ingestOperation.current = true;
      requestGeneration.current += 1;
      const target = source ?? "all";
      setRebuilding(target);
      setBusy(true);
      setStatus(`${source ? `${source} ` : "全部"}缓存重建中…`);
      try {
        const report = await invoke<IngestReport>("rebuild_cache", { source });
        dataEpoch.current += 1;
        setSessionsRevision((n) => n + 1);
        setLastIngestReport(report);
        const archived = report.records_archived > 0 ? `，归档 ${report.records_archived}` : "";
        setStatus(
          `缓存重建${report.partial_success ? "部分完成" : "完成"}：写入 ${report.records_written}${archived}，清理 ${report.records_removed}，失败 ${report.files_failed}`,
        );
        await refreshViews();
        try {
          await invoke("refresh_tray");
        } catch {
          // 菜单栏刷新失败不阻断主界面
        }
      } catch (error) {
        setStatus(`缓存重建失败：${humanStatus(error)}`);
        setLoading(false);
      } finally {
        ingestOperation.current = false;
        setRebuilding(null);
        setBusy(false);
      }
    },
    [refreshViews],
  );

  const runPurgeArchived = useCallback(
    async (source: string | null) => {
      if (ingestOperation.current) {
        return;
      }
      ingestOperation.current = true;
      const target = source ?? "all";
      setPurging(target);
      setBusy(true);
      setStatus(`正在清理${source ? `${source} ` : "全部"}已归档记录…`);
      try {
        const removed = await invoke<number>("purge_archived_records", { source });
        dataEpoch.current += 1;
        setSessionsRevision((n) => n + 1);
        setStatus(`已永久删除 ${removed} 条归档记录`);
        await refreshViews();
      } catch (error) {
        setStatus(`清理归档记录失败：${humanStatus(error)}`);
      } finally {
        ingestOperation.current = false;
        setPurging(null);
        setBusy(false);
      }
    },
    [refreshViews],
  );

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

  useEffect(() => {
    invoke<string>("ping")
      .then(async () => {
        setConnected(true);
        setStatus("正在加载缓存…");
        try {
          // 先画已有缓存。启动摄取可能扫数 GB 源文件，不能挡住首屏。
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

  const selectSession = useCallback(
    (session: SelectedSession) => {
      setSelectedSession(session);
      loadSessionTurns(session).catch(reportError);
    },
    [loadSessionTurns, reportError],
  );

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
    runIngest,
    runRebuild,
    runPurgeArchived,
    reportError,
  };
}
