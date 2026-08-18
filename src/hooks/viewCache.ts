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

export function parseViewHash(raw: string): View {
  const hash = raw.replace(/^#/, "");
  if (hash === "source") {
    return "application";
  }
  if (hash === "settings" || hash.startsWith("settings-")) {
    return "settings";
  }
  return views.find((item) => item === hash) ?? "overview";
}

export function viewFromHash(): View {
  return parseViewHash(window.location.hash);
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
