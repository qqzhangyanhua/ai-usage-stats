import type { Filter, Grain, View } from "../types";
import { emptyFilter } from "./usage/constants";

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
  "worktime",
  "instructions",
  "settings",
];

export type ViewScope = {
  filter: Filter;
  preset: string;
};

export function emptyViewScope(): ViewScope {
  return {
    filter: {
      ...emptyFilter,
      sources: [],
      models: [],
      projects: [],
      providers: [],
    },
    preset: "all",
  };
}

export function initialViewScopes(): Record<View, ViewScope> {
  return Object.fromEntries(views.map((view) => [view, emptyViewScope()])) as Record<
    View,
    ViewScope
  >;
}

function sameItems(left: string[], right: string[]): boolean {
  if (left.length !== right.length) {
    return false;
  }
  const other = new Set(right);
  return left.every((item) => other.has(item));
}

export function filtersEqual(left: Filter, right: Filter): boolean {
  return (
    left.from === right.from &&
    left.to === right.to &&
    sameItems(left.sources, right.sources) &&
    sameItems(left.models, right.models) &&
    sameItems(left.projects, right.projects) &&
    sameItems(left.providers, right.providers)
  );
}

export function scopesEqual(left: ViewScope, right: ViewScope): boolean {
  return left.preset === right.preset && filtersEqual(left.filter, right.filter);
}

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

/** 本次查询会覆盖这些视图正在展示的共享数据集。 */
export function viewsInvalidatedBy(view: View): View[] {
  switch (view) {
    case "overview":
      return ["trend", "model", "project"];
    case "trend":
    case "model":
    case "project":
      return ["overview"];
    default:
      return [];
  }
}

/**
 * 按「各视图独立筛选」校正缓存戳：
 * - 本次查询筛选用过的视图标为新鲜
 * - 筛选用过且一致的预热页也可以复用
 * - 共享数据被不同筛选覆盖的兄弟页必须失效
 */
export function reconcileLoadedStamps(
  loaded: Partial<Record<View, string>>,
  target: View,
  used: ViewScope,
  scopes: Record<View, ViewScope>,
  grain: Grain,
  epoch: number,
): Partial<Record<View, string>> {
  const next = { ...loaded };
  for (const view of viewsWarmedBy(target)) {
    const scope = view === target ? used : scopes[view];
    if (scopesEqual(scope, used)) {
      next[view] = viewStamp(view, scope.filter, scope.preset, grain, epoch);
    }
  }
  for (const view of viewsInvalidatedBy(target)) {
    if (!scopesEqual(scopes[view], used)) {
      delete next[view];
    }
  }
  return next;
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
