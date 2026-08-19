export const OVERVIEW_LAYOUT_STORAGE_KEY = "ai-usage-stats:overview-layout";

export const OVERVIEW_MODULE_IDS = [
  "kpi",
  "billing",
  "weekly",
  "trend",
  "heatmap",
  "detail",
  "status",
] as const;

export type OverviewModuleId = (typeof OVERVIEW_MODULE_IDS)[number];

export const OVERVIEW_MODULE_LABELS: Record<OverviewModuleId, string> = {
  kpi: "指标卡片",
  billing: "5 小时计费窗",
  weekly: "滚动用量",
  trend: "趋势与模型",
  heatmap: "活跃热力图",
  detail: "明细",
  status: "底部状态",
};

/** 额度模块（计费窗 / 滚动用量）可单独开关的来源，常用项靠前。 */
export const QUOTA_SOURCE_IDS = [
  "codex",
  "claude",
  "cursor_agent",
  "copilot",
  "factory",
  "pi",
  "opencode",
  "kimi",
  "dsh",
  "gemini",
  "grok",
  "qwen",
] as const;

export type QuotaSourceId = (typeof QUOTA_SOURCE_IDS)[number];

/** 额度区块里最常单独盯的来源，对应「常用」一键。 */
export const FAVORITE_QUOTA_SOURCES = ["codex", "claude", "cursor_agent"] as const;

export type OverviewLayout = {
  modules: Record<OverviewModuleId, boolean>;
  quotaSources: Record<string, boolean>;
};

export type OverviewLayoutSummary = {
  hiddenModules: OverviewModuleId[];
  hiddenPresentSources: string[];
};

export function defaultOverviewLayout(): OverviewLayout {
  return {
    modules: {
      kpi: true,
      billing: true,
      weekly: true,
      trend: true,
      heatmap: true,
      detail: true,
      status: true,
    },
    quotaSources: Object.fromEntries(QUOTA_SOURCE_IDS.map((id) => [id, true])),
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readFlag(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

export function parseOverviewLayout(raw: string | null): OverviewLayout {
  const defaults = defaultOverviewLayout();
  if (raw == null || raw === "") {
    return defaults;
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!isRecord(parsed)) {
      return defaults;
    }
    const modulesRaw = isRecord(parsed.modules) ? parsed.modules : {};
    const sourcesRaw = isRecord(parsed.quotaSources) ? parsed.quotaSources : {};
    const modules = { ...defaults.modules };
    for (const id of OVERVIEW_MODULE_IDS) {
      modules[id] = readFlag(modulesRaw[id], defaults.modules[id]);
    }
    const quotaSources = { ...defaults.quotaSources };
    for (const [source, visible] of Object.entries(sourcesRaw)) {
      if (typeof source === "string" && source.length > 0) {
        quotaSources[source] = readFlag(visible, true);
      }
    }
    return { modules, quotaSources };
  } catch {
    return defaults;
  }
}

export function readOverviewLayout(): OverviewLayout {
  try {
    return parseOverviewLayout(localStorage.getItem(OVERVIEW_LAYOUT_STORAGE_KEY));
  } catch {
    return defaultOverviewLayout();
  }
}

export function writeOverviewLayout(layout: OverviewLayout): void {
  try {
    localStorage.setItem(OVERVIEW_LAYOUT_STORAGE_KEY, JSON.stringify(layout));
  } catch {
    /* quota / private mode */
  }
}

export function isModuleVisible(layout: OverviewLayout, id: OverviewModuleId): boolean {
  return layout.modules[id] !== false;
}

export function isQuotaSourceVisible(layout: OverviewLayout, source: string): boolean {
  return layout.quotaSources[source] !== false;
}

export function filterQuotaItems<T extends { source: string }>(
  items: T[],
  layout: OverviewLayout,
): T[] {
  return items.filter((item) => isQuotaSourceVisible(layout, item.source));
}

export function setModuleVisible(
  layout: OverviewLayout,
  id: OverviewModuleId,
  visible: boolean,
): OverviewLayout {
  return {
    ...layout,
    modules: { ...layout.modules, [id]: visible },
  };
}

export function setQuotaSourceVisible(
  layout: OverviewLayout,
  source: string,
  visible: boolean,
): OverviewLayout {
  return {
    ...layout,
    quotaSources: { ...layout.quotaSources, [source]: visible },
  };
}

export function setAllModulesVisible(layout: OverviewLayout, visible: boolean): OverviewLayout {
  const modules = { ...layout.modules };
  for (const id of OVERVIEW_MODULE_IDS) {
    modules[id] = visible;
  }
  return { ...layout, modules };
}

export function setAllQuotaSourcesVisible(
  layout: OverviewLayout,
  visible: boolean,
): OverviewLayout {
  const quotaSources = { ...layout.quotaSources };
  for (const id of QUOTA_SOURCE_IDS) {
    quotaSources[id] = visible;
  }
  for (const source of Object.keys(quotaSources)) {
    quotaSources[source] = visible;
  }
  return { ...layout, quotaSources };
}

export function visibleModuleCount(layout: OverviewLayout): number {
  return OVERVIEW_MODULE_IDS.filter((id) => isModuleVisible(layout, id)).length;
}

export function visibleQuotaSourceCount(layout: OverviewLayout): number {
  return QUOTA_SOURCE_IDS.filter((id) => isQuotaSourceVisible(layout, id)).length;
}

export function applyQuotaSourceSet(
  layout: OverviewLayout,
  enabled: readonly string[],
): OverviewLayout {
  const allow = new Set(enabled);
  const quotaSources = { ...layout.quotaSources };
  for (const id of QUOTA_SOURCE_IDS) {
    quotaSources[id] = allow.has(id);
  }
  for (const source of Object.keys(quotaSources)) {
    quotaSources[source] = allow.has(source);
  }
  return { ...layout, quotaSources };
}

export function applyFavoriteQuotaSources(layout: OverviewLayout): OverviewLayout {
  return applyQuotaSourceSet(layout, FAVORITE_QUOTA_SOURCES);
}

export function applyDetectedQuotaSources(
  layout: OverviewLayout,
  detectedSources: readonly string[],
): OverviewLayout {
  return applyQuotaSourceSet(layout, detectedSources);
}

export function collectPresentSources(
  detectedSources: readonly string[],
  items: readonly { source: string }[],
): string[] {
  const present = new Set<string>();
  for (const source of detectedSources) {
    if (source) {
      present.add(source);
    }
  }
  for (const item of items) {
    if (item.source) {
      present.add(item.source);
    }
  }
  const known = QUOTA_SOURCE_IDS.filter((id) => present.has(id));
  const extra = [...present].filter(
    (source) => !QUOTA_SOURCE_IDS.includes(source as QuotaSourceId),
  );
  return [...known, ...extra];
}

export function quotaSourceChipIds(presentSources: readonly string[], showAll: boolean): string[] {
  if (showAll || presentSources.length === 0) {
    const extra = presentSources.filter(
      (source) => !QUOTA_SOURCE_IDS.includes(source as QuotaSourceId),
    );
    return [...QUOTA_SOURCE_IDS, ...extra];
  }
  return [...presentSources];
}

export function summarizeOverviewLayout(
  layout: OverviewLayout,
  presentSources: readonly string[] = [],
): OverviewLayoutSummary {
  const hiddenModules = OVERVIEW_MODULE_IDS.filter((id) => !isModuleVisible(layout, id));
  const sourcePool = presentSources.length > 0 ? presentSources : QUOTA_SOURCE_IDS;
  const hiddenPresentSources = sourcePool.filter((source) => !isQuotaSourceVisible(layout, source));
  return { hiddenModules, hiddenPresentSources };
}
