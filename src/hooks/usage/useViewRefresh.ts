import { invoke } from "@tauri-apps/api/core";
import {
  useCallback,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";
import { heatmapFilter } from "../../lib/calendar";
import { previousFilter } from "../../lib/format";
import type {
  ApplicationAnalyticsDto,
  BillingWindowsDto,
  BudgetStatusDto,
  CodeVolumeSummary,
  CursorAccountUsageDto,
  CursorSessionSummaryDto,
  Filter,
  FilterOptions,
  Grain,
  NamedAmount,
  OfficialQuotaDto,
  OverviewDto,
  PriceTable,
  SeriesPoint,
  SessionRow,
  SourceDiagnostic,
  View,
} from "../../types";
import { isViewFresh, viewStamp } from "../viewCache";

type ViewRefreshArgs = {
  view: View;
  filter: Filter;
  preset: string;
  grain: Grain;
  hydratedViews: Set<View>;
  requestGenerationRef: MutableRefObject<number>;
  dataEpochRef: MutableRefObject<number>;
  loadedStampsRef: MutableRefObject<Partial<Record<View, string>>>;
  optionsEpochRef: MutableRefObject<number>;
  markHydrated: (target: View, nextFilter: Filter, nextPreset: string) => void;
  setLoading: Dispatch<SetStateAction<boolean>>;
  setOptions: Dispatch<SetStateAction<FilterOptions>>;
  setOverview: Dispatch<SetStateAction<OverviewDto | null>>;
  setTrend: Dispatch<SetStateAction<SeriesPoint[]>>;
  setModels: Dispatch<SetStateAction<NamedAmount[]>>;
  setProjects: Dispatch<SetStateAction<NamedAmount[]>>;
  setSessions: Dispatch<SetStateAction<SessionRow[]>>;
  setBillingWindows: Dispatch<SetStateAction<BillingWindowsDto | null>>;
  setOfficialQuota: Dispatch<SetStateAction<OfficialQuotaDto | null>>;
  setCursorAccountUsage: Dispatch<SetStateAction<CursorAccountUsageDto | null>>;
  setBudgetStatus: Dispatch<SetStateAction<BudgetStatusDto | null>>;
  setHeatmap: Dispatch<SetStateAction<SeriesPoint[]>>;
  setHeatmapRange: Dispatch<SetStateAction<{ from: string; to: string }>>;
  setPrevious: Dispatch<SetStateAction<OverviewDto | null>>;
  setApplicationAnalytics: Dispatch<SetStateAction<ApplicationAnalyticsDto | null>>;
  setProviderBreakdown: Dispatch<SetStateAction<NamedAmount[]>>;
  setCodeVolume: Dispatch<SetStateAction<CodeVolumeSummary | null>>;
  setCodeVolumeLoading: Dispatch<SetStateAction<boolean>>;
  setCursorSessionSummary: Dispatch<SetStateAction<CursorSessionSummaryDto | null>>;
  setCursorSessionLoading: Dispatch<SetStateAction<boolean>>;
  setPrices: Dispatch<SetStateAction<PriceTable>>;
  setDiagnostics: Dispatch<SetStateAction<SourceDiagnostic[]>>;
  setUpdatedAt: Dispatch<SetStateAction<string | null>>;
};

export function useViewRefresh(args: ViewRefreshArgs) {
  const {
    view,
    filter,
    preset,
    grain,
    hydratedViews,
    requestGenerationRef,
    dataEpochRef,
    loadedStampsRef,
    optionsEpochRef,
    markHydrated,
    setLoading,
    setOptions,
    setOverview,
    setTrend,
    setModels,
    setProjects,
    setSessions,
    setBillingWindows,
    setOfficialQuota,
    setCursorAccountUsage,
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
  } = args;

  const refreshViews = useCallback(
    async (nextFilter = filter, nextPreset = preset) => {
      const generation = ++requestGenerationRef.current;
      const localOnly =
        view === "conversations" ||
        view === "cursor" ||
        view === "cursor-sessions" ||
        view === "worktime" ||
        view === "instructions" ||
        view === "settings";
      if (!localOnly && !hydratedViews.has(view)) {
        setLoading(true);
      }
      const commit =
        <T>(setter: (value: T) => void) =>
        (value: T) => {
          if (generation === requestGenerationRef.current) {
            setter(value);
          }
        };
      const epoch = dataEpochRef.current;
      const overviewFresh = isViewFresh(
        loadedStampsRef.current,
        "overview",
        nextFilter,
        nextPreset,
        grain,
        epoch,
      );
      const paint: Array<Promise<void>> = [];
      if (optionsEpochRef.current !== epoch) {
        paint.push(
          invoke<FilterOptions>("get_filter_options").then((value) => {
            commit(setOptions)(value);
            if (generation === requestGenerationRef.current) {
              optionsEpochRef.current = epoch;
            }
          }),
        );
      }
      if (view !== "conversations" && !overviewFresh) {
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
          invoke<OfficialQuotaDto>("get_official_quota").then((value) => {
            commit(setOfficialQuota)(value);
            void invoke<OfficialQuotaDto>("refresh_official_quota")
              .then(commit(setOfficialQuota))
              .catch(() => undefined);
          }),
          invoke<CursorAccountUsageDto>("get_cursor_account_usage", {
            filter: nextFilter,
          }).then(commit(setCursorAccountUsage)),
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
        } else if (generation === requestGenerationRef.current) {
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
      if (view === "cursor") {
        setCodeVolumeLoading(true);
        tasks.push(
          invoke<CodeVolumeSummary>("get_code_volume")
            .then(commit(setCodeVolume))
            .finally(() => {
              if (generation === requestGenerationRef.current) {
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
              if (generation === requestGenerationRef.current) {
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
          generation === requestGenerationRef.current &&
          (view === "overview" || view === "cursor" || view === "cursor-sessions")
        ) {
          setLoading(false);
        }
        if (tasks.length > 0) {
          await Promise.all(tasks);
        }
        if (generation === requestGenerationRef.current) {
          markHydrated(view, nextFilter, nextPreset);
        }
      } finally {
        if (generation === requestGenerationRef.current) {
          setLoading(false);
        }
      }
      if (generation === requestGenerationRef.current) {
        setUpdatedAt(new Date().toISOString());
      }
    },
    [
      view,
      filter,
      preset,
      grain,
      hydratedViews,
      markHydrated,
      requestGenerationRef,
      dataEpochRef,
      loadedStampsRef,
      optionsEpochRef,
      setLoading,
      setOptions,
      setOverview,
      setTrend,
      setModels,
      setProjects,
      setSessions,
      setBillingWindows,
      setOfficialQuota,
      setCursorAccountUsage,
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
    ],
  );

  const refreshTrend = useCallback(
    async (nextFilter = filter) => {
      const generation = ++requestGenerationRef.current;
      const commit =
        <T>(setter: (value: T) => void) =>
        (value: T) => {
          if (generation === requestGenerationRef.current) {
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
        if (generation === requestGenerationRef.current) {
          setLoading(false);
          if (view === "overview" || view === "trend") {
            markHydrated("trend", nextFilter, preset);
            loadedStampsRef.current.overview = viewStamp(
              "overview",
              nextFilter,
              preset,
              grain,
              dataEpochRef.current,
            );
          }
          if (view === "application") {
            markHydrated("application", nextFilter, preset);
          }
        }
      }
      if (generation === requestGenerationRef.current) {
        setUpdatedAt(new Date().toISOString());
      }
    },
    [
      view,
      filter,
      preset,
      grain,
      markHydrated,
      requestGenerationRef,
      dataEpochRef,
      loadedStampsRef,
      setLoading,
      setTrend,
      setApplicationAnalytics,
      setUpdatedAt,
    ],
  );

  return { refreshViews, refreshTrend };
}
