import { invoke } from "@tauri-apps/api/core";
import { Suspense } from "react";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { LoadingOverlay } from "./components/LoadingOverlay";
import { Sidebar } from "./components/Sidebar";
import { Topbar } from "./components/Topbar";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { useOverviewLayout } from "./hooks/useOverviewLayout";
import { useTheme } from "./hooks/useTheme";
import { useUsageData } from "./hooks/useUsageData";
import { clearDimensionFilters, withModelFilter } from "./lib/filterChips";
import {
  LazyApplicationAnalytics,
  LazyBreakdown,
  LazyCursorAccountUsagePanel,
  LazyCursorPanel,
  LazyCursorSessionPanel,
  LazyGlobalInstructionPanel,
  LazyOverview,
  LazySessions,
  LazySettings,
  LazyTrend,
} from "./views/lazyViews";
import { ViewFallback } from "./views/ViewFallback";

export default function App() {
  const data = useUsageData();
  const { theme, mode: themeMode, setMode: setThemeMode } = useTheme();
  const { layout: overviewLayout, setLayout: setOverviewLayout } = useOverviewLayout();
  const { view } = data;
  const detectedSources = data.diagnostics.filter((row) => row.detected).map((row) => row.source);

  useKeyboardShortcuts({
    onNavigate: data.navigate,
    onRefresh: () => {
      void data.runIngest("刷新");
    },
    onClearFilters: () => data.applyFilter(clearDimensionFilters(data.filter)),
  });

  return (
    <div className="app">
      <Sidebar
        view={view}
        busy={data.busy}
        connected={data.connected}
        status={data.status}
        onNavigate={data.navigate}
      />
      <div className="workspace">
        <Topbar
          key={view}
          view={view}
          filter={data.filter}
          preset={data.preset}
          options={data.options}
          disabled={data.loading}
          refreshDisabled={data.busy}
          onPreset={data.applyPreset}
          onChange={data.applyFilter}
          onRangeBack={data.canGoBack ? data.popRange : undefined}
          onRefresh={() => data.runIngest("刷新")}
        />
        <main className="main">
          <LoadingOverlay
            active={
              data.loading &&
              !data.viewHasData &&
              view !== "sessions" &&
              view !== "cursor" &&
              view !== "cursor-sessions" &&
              view !== "instructions"
            }
          >
            <ErrorBoundary fullscreen={false}>
              <Suspense fallback={<ViewFallback />}>
                {view === "overview" ? (
                  <LazyOverview
                    overview={data.overview}
                    billingWindows={data.billingWindows}
                    officialQuota={data.officialQuota}
                    previous={data.previous}
                    trend={data.trend}
                    heatmap={data.heatmap}
                    heatmapRange={data.heatmapRange}
                    models={data.models}
                    projects={data.projects}
                    sessions={data.sessions}
                    grain={data.grain}
                    preset={data.preset}
                    updatedAt={data.updatedAt}
                    live={data.connected}
                    theme={theme}
                    onGrain={data.setGrain}
                    onOpenSessions={data.openSessions}
                    onProjectClick={(project) =>
                      data.applyFilter({ ...data.filter, projects: [project] })
                    }
                    onRangeSelect={data.drillRange}
                    onRangeBack={data.canGoBack ? data.popRange : undefined}
                    onModelClick={(model) => data.applyFilter(withModelFilter(data.filter, model))}
                    onSessionClick={(session) => {
                      data.openSessions();
                      data.setSelectedSession(session);
                    }}
                    layout={overviewLayout}
                    onLayoutChange={setOverviewLayout}
                    detectedSources={detectedSources}
                  />
                ) : null}
                {view === "trend" ? (
                  <LazyTrend
                    grain={data.grain}
                    setGrain={data.setGrain}
                    points={data.trend}
                    theme={theme}
                    onRangeSelect={data.drillRange}
                    onRangeBack={data.canGoBack ? data.popRange : undefined}
                  />
                ) : null}
                {view === "application" ? (
                  <LazyApplicationAnalytics
                    analytics={data.applicationAnalytics}
                    grain={data.grain}
                    setGrain={data.setGrain}
                    theme={theme}
                  />
                ) : null}
                {["model", "provider", "project"].includes(view) ? (
                  <LazyBreakdown
                    title={
                      view === "model" ? "按模型" : view === "provider" ? "按 provider" : "按项目"
                    }
                    icon={view === "model" ? "model" : view === "provider" ? "provider" : "project"}
                    rows={data.breakdown}
                    showProviderChannel={view === "provider"}
                    showVendorIcon={view === "model" || view === "provider"}
                    projectNames={view === "project"}
                    theme={theme}
                  />
                ) : null}
                {view === "cursor" ? (
                  <div className="stack">
                    <LazyCursorAccountUsagePanel theme={theme} />
                    <LazyCursorPanel
                      summary={data.codeVolume}
                      loading={data.codeVolumeLoading}
                      theme={theme}
                    />
                  </div>
                ) : null}
                {view === "cursor-sessions" ? (
                  <LazyCursorSessionPanel
                    summary={data.cursorSessionSummary}
                    loading={data.cursorSessionLoading}
                    theme={theme}
                    revision={data.sessionsRevision}
                    onError={data.reportError}
                  />
                ) : null}
                {view === "instructions" ? <LazyGlobalInstructionPanel /> : null}
                {view === "settings" ? (
                  <LazySettings
                    prices={data.prices}
                    diagnostics={data.diagnostics}
                    ingestReport={data.lastIngestReport}
                    rebuilding={data.rebuilding}
                    purging={data.purging}
                    operationBusy={data.busy}
                    observedModels={data.options.models}
                    budgetStatus={data.budgetStatus}
                    savingBudget={data.savingBudget}
                    onChange={data.setPrices}
                    onRebuild={data.runRebuild}
                    onPurgeArchived={data.runPurgeArchived}
                    onSave={async () => {
                      try {
                        await invoke("save_price_table", { prices: data.prices });
                        data.setStatus("单价已保存");
                      } catch (error) {
                        data.reportError(error);
                      }
                    }}
                    onSnapshotRefreshed={() => data.runIngest("刷新")}
                    onSaveBudget={(monthlyUsd: number | null) =>
                      data.saveBudget({ monthly_usd: monthlyUsd }).catch(() => undefined)
                    }
                    officialQuota={data.officialQuota}
                    onOfficialQuota={data.setOfficialQuota}
                    onQuotaError={data.reportError}
                    overviewLayout={overviewLayout}
                    onOverviewLayoutChange={setOverviewLayout}
                    themeMode={themeMode}
                    autoRefresh={data.autoRefresh}
                    onThemeModeChange={setThemeMode}
                    onAutoRefreshChange={data.setAutoRefresh}
                  />
                ) : null}
              </Suspense>
            </ErrorBoundary>
            {data.sessionsVisited ? (
              <div hidden={view !== "sessions"}>
                <ErrorBoundary fullscreen={false}>
                  <Suspense fallback={<ViewFallback />}>
                    <LazySessions
                      filter={data.sessionsFilter}
                      options={data.options}
                      revision={data.sessionsRevision}
                      turns={data.turns}
                      turnsLoading={data.turnsLoading}
                      selected={data.selectedSession}
                      onSelect={data.setSelectedSession}
                      onFilterChange={data.applySessionsFilter}
                      onError={data.reportError}
                    />
                  </Suspense>
                </ErrorBoundary>
              </div>
            ) : null}
          </LoadingOverlay>
        </main>
      </div>
    </div>
  );
}
