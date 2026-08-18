import type { Filter, Grain, View } from "../types";

export const views: View[] = [
  "overview",
  "trend",
  "application",
  "model",
  "provider",
  "project",
  "sessions",
  "cursor",
  "cursor-sessions",
  "settings",
];

export function viewFromHash(): View {
  const raw = window.location.hash.replace(/^#/, "");
  if (raw === "source") {
    return "application";
  }
  return views.find((item) => item === raw) ?? "overview";
}

export function viewStamp(
  view: View,
  filter: Filter,
  preset: string,
  grain: Grain,
  epoch: number,
): string {
  const grainSensitive = view === "overview" || view === "trend" || view === "application";
  return JSON.stringify({
    epoch,
    preset,
    from: filter.from,
    to: filter.to,
    sources: filter.sources,
    models: filter.models,
    projects: filter.projects,
    providers: filter.providers,
    grain: grainSensitive ? grain : "",
  });
}

/** 拉概览时已经带上了趋势、模型、项目，切到这些页不应再打一轮查询。 */
export function viewsWarmedBy(view: View): View[] {
  if (view === "overview") {
    return ["overview", "trend", "model", "project"];
  }
  return [view];
}

export function isViewFresh(
  loaded: Partial<Record<View, string>>,
  view: View,
  filter: Filter,
  preset: string,
  grain: Grain,
  epoch: number,
): boolean {
  return loaded[view] === viewStamp(view, filter, preset, grain, epoch);
}
