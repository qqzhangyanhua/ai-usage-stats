import { invoke } from "@tauri-apps/api/core";
import { ApplicationAnalytics } from "./components/ApplicationAnalytics";
import { Breakdown } from "./components/Breakdown";
import { CursorAccountUsagePanel } from "./components/CursorAccountUsagePanel";
import { CursorPanel } from "./components/CursorPanel";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { LoadingOverlay } from "./components/LoadingOverlay";
import { Overview } from "./components/Overview";
import { Sessions } from "./components/Sessions";
import { Settings } from "./components/Settings";
import { Sidebar } from "./components/Sidebar";
import { Topbar } from "./components/Topbar";
import { Trend } from "./components/Trend";
import { useTheme } from "./hooks/useTheme";
import { useUsageData } from "./hooks/useUsageData";

export default function App() {
  const data = useUsageData();
  const { theme, mode: themeMode, setMode: setThemeMode } = useTheme();
  const { view } = data;

  return (
    <div className="app">
      <Sidebar
        view={view}
        busy={data.busy}
        connected={data.connected}
        status={data.status}
        autoRefresh={data.autoRefresh}
        themeMode={themeMode}
        onAutoRefreshChange={data.setAutoRefresh}
        onNavigate={data.navigate}
        onThemeModeChange={setThemeMode}
      />
      <div className="workspace">
        <Topbar
          view={view}
          filter={data.filter}
          preset={data.preset}
          options={data.options}
          disabled={data.busy}
          onPreset={data.applyPreset}
          onChange={data.applyFilter}
          onRefresh={() => data.runIngest("刷新")}
        />
        <main className="main">
          <LoadingOverlay active={data.loading || data.busy}>
            <ErrorBoundary key={view} fullscreen={false}>
              {view === "overview" ? (
                <Overview
                  overview={data.overview}
                  billingWindows={data.billingWindows}
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
                />
              ) : null}
              {view === "trend" ? (
                <Trend
                  grain={data.grain}
                  setGrain={data.setGrain}
                  points={data.trend}
                  theme={theme}
                />
              ) : null}
              {view === "application" ? (
                <ApplicationAnalytics
                  analytics={data.applicationAnalytics}
                  grain={data.grain}
                  setGrain={data.setGrain}
                  theme={theme}
                />
              ) : null}
              {["model", "provider", "project"].includes(view) ? (
                <Breakdown
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
                  <CursorAccountUsagePanel theme={theme} />
                  <CursorPanel summary={data.codeVolume} theme={theme} />
                </div>
              ) : null}
              {view === "settings" ? (
                <Settings
                  prices={data.prices}
                  diagnostics={data.diagnostics}
                  ingestReport={data.lastIngestReport}
                  rebuilding={data.rebuilding}
                  operationBusy={data.busy}
                  observedModels={data.options.models}
                  onChange={data.setPrices}
                  onRebuild={data.runRebuild}
                  onSave={async () => {
                    await invoke("save_price_table", { prices: data.prices });
                    data.setStatus("单价已保存");
                  }}
                  onSnapshotRefreshed={() => data.runIngest("刷新")}
                />
              ) : null}
            </ErrorBoundary>
            {data.sessionsVisited ? (
              <div hidden={view !== "sessions"}>
                <ErrorBoundary fullscreen={false}>
                  <Sessions
                    filter={data.filter}
                    options={data.options}
                    revision={data.sessionsRevision}
                    turns={data.turns}
                    turnsLoading={data.turnsLoading}
                    selected={data.selectedSession}
                    onSelect={data.setSelectedSession}
                    onFilterChange={data.applyFilter}
                  />
                </ErrorBoundary>
              </div>
            ) : null}
          </LoadingOverlay>
        </main>
      </div>
    </div>
  );
}
