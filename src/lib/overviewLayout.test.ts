import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  applyDetectedQuotaSources,
  applyFavoriteQuotaSources,
  collectPresentSources,
  defaultOverviewLayout,
  filterQuotaItems,
  isModuleVisible,
  isQuotaSourceVisible,
  OVERVIEW_LAYOUT_STORAGE_KEY,
  OVERVIEW_MODULE_IDS,
  parseOverviewLayout,
  QUOTA_SOURCE_IDS,
  quotaSourceChipIds,
  readOverviewLayout,
  setAllModulesVisible,
  setAllQuotaSourcesVisible,
  setModuleVisible,
  setQuotaSourceVisible,
  summarizeOverviewLayout,
  visibleModuleCount,
  visibleQuotaSourceCount,
  writeOverviewLayout,
} from "./overviewLayout";

function installMemoryStorage() {
  const store = new Map<string, string>();
  const memory: Storage = {
    get length() {
      return store.size;
    },
    clear() {
      store.clear();
    },
    getItem(key) {
      return store.get(key) ?? null;
    },
    key(index) {
      return [...store.keys()][index] ?? null;
    },
    removeItem(key) {
      store.delete(key);
    },
    setItem(key, value) {
      store.set(key, value);
    },
  };
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: memory,
  });
}

describe("parseOverviewLayout", () => {
  it("returns all-visible defaults for empty input", () => {
    const layout = parseOverviewLayout(null);
    expect(layout).toEqual(defaultOverviewLayout());
    expect(OVERVIEW_MODULE_IDS.every((id) => layout.modules[id])).toBe(true);
    expect(QUOTA_SOURCE_IDS.every((id) => layout.quotaSources[id])).toBe(true);
  });

  it("merges partial stored config and keeps unknown sources", () => {
    const layout = parseOverviewLayout(
      JSON.stringify({
        modules: { heatmap: false, kpi: true },
        quotaSources: { codex: true, claude: false, custom_src: false },
      }),
    );
    expect(layout.modules.heatmap).toBe(false);
    expect(layout.modules.official).toBe(true);
    expect(layout.modules.billing).toBe(true);
    expect(layout.quotaSources.codex).toBe(true);
    expect(layout.quotaSources.claude).toBe(false);
    expect(layout.quotaSources.cursor_agent).toBe(true);
    expect(layout.quotaSources.custom_src).toBe(false);
  });

  it("falls back for invalid JSON or non-object payloads", () => {
    expect(parseOverviewLayout("{")).toEqual(defaultOverviewLayout());
    expect(parseOverviewLayout("[]")).toEqual(defaultOverviewLayout());
    expect(parseOverviewLayout('"nope"')).toEqual(defaultOverviewLayout());
  });
});

describe("visibility helpers", () => {
  it("treats missing flags as visible", () => {
    const layout = defaultOverviewLayout();
    layout.quotaSources = { codex: false };
    expect(isModuleVisible(layout, "weekly")).toBe(true);
    expect(isQuotaSourceVisible(layout, "codex")).toBe(false);
    expect(isQuotaSourceVisible(layout, "cursor_agent")).toBe(true);
    expect(isQuotaSourceVisible(layout, "unknown")).toBe(true);
  });

  it("filters quota rows by configured sources", () => {
    const layout = setQuotaSourceVisible(defaultOverviewLayout(), "claude", false);
    const rows = filterQuotaItems(
      [
        { source: "codex", total: 1 },
        { source: "claude", total: 2 },
        { source: "cursor_agent", total: 3 },
      ],
      layout,
    );
    expect(rows.map((row) => row.source)).toEqual(["codex", "cursor_agent"]);
  });

  it("toggles modules and sources without mutating the original", () => {
    const original = defaultOverviewLayout();
    const hiddenHeatmap = setModuleVisible(original, "heatmap", false);
    const hiddenCodex = setQuotaSourceVisible(original, "codex", false);
    expect(original.modules.heatmap).toBe(true);
    expect(original.quotaSources.codex).toBe(true);
    expect(hiddenHeatmap.modules.heatmap).toBe(false);
    expect(hiddenCodex.quotaSources.codex).toBe(false);
  });

  it("supports show/hide all and counts visible items", () => {
    const hidden = setAllModulesVisible(defaultOverviewLayout(), false);
    const shown = setAllQuotaSourcesVisible(setAllQuotaSourcesVisible(hidden, false), true);
    expect(visibleModuleCount(hidden)).toBe(0);
    expect(visibleQuotaSourceCount(shown)).toBe(QUOTA_SOURCE_IDS.length);
    expect(shown.modules.kpi).toBe(false);
  });

  it("applies favorite and detected source sets", () => {
    const favorites = applyFavoriteQuotaSources(defaultOverviewLayout());
    expect(favorites.quotaSources.codex).toBe(true);
    expect(favorites.quotaSources.cursor_agent).toBe(true);
    expect(favorites.quotaSources.grok).toBe(false);
    const detected = applyDetectedQuotaSources(favorites, ["codex", "kimi"]);
    expect(detected.quotaSources.codex).toBe(true);
    expect(detected.quotaSources.cursor_agent).toBe(false);
    expect(detected.quotaSources.kimi).toBe(true);
  });

  it("lists present sources and collapses chips until show-all", () => {
    const present = collectPresentSources(["kimi"], [{ source: "codex" }, { source: "custom" }]);
    expect(present).toEqual(["codex", "kimi", "custom"]);
    expect(quotaSourceChipIds(present, false)).toEqual(["codex", "kimi", "custom"]);
    expect(quotaSourceChipIds(present, true)[0]).toBe("codex");
    expect(quotaSourceChipIds(present, true)).toContain("cursor_agent");
    expect(quotaSourceChipIds([], false)).toEqual([...QUOTA_SOURCE_IDS]);
  });

  it("summarizes hidden modules and present sources", () => {
    const layout = setModuleVisible(
      setQuotaSourceVisible(defaultOverviewLayout(), "claude", false),
      "heatmap",
      false,
    );
    const summary = summarizeOverviewLayout(layout, ["codex", "claude", "cursor_agent"]);
    expect(summary.hiddenModules).toEqual(["heatmap"]);
    expect(summary.hiddenPresentSources).toEqual(["claude"]);
  });
});

describe("readOverviewLayout / writeOverviewLayout", () => {
  beforeEach(() => {
    installMemoryStorage();
  });

  afterEach(() => {
    localStorage.removeItem(OVERVIEW_LAYOUT_STORAGE_KEY);
  });

  it("persists and reloads a customized layout", () => {
    const next = setModuleVisible(
      setQuotaSourceVisible(defaultOverviewLayout(), "cursor_agent", false),
      "billing",
      false,
    );
    writeOverviewLayout(next);
    const loaded = readOverviewLayout();
    expect(loaded.modules.billing).toBe(false);
    expect(loaded.quotaSources.cursor_agent).toBe(false);
    expect(loaded.modules.weekly).toBe(true);
  });
});
