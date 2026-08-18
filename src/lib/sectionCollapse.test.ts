import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  parseSectionOpen,
  readSectionOpen,
  sectionStorageKey,
  writeSectionOpen,
} from "./sectionCollapse";

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

describe("sectionStorageKey", () => {
  it("prefixes the overview section id", () => {
    expect(sectionStorageKey("billing")).toBe("ai-usage-stats:overview-section:billing");
  });
});

describe("parseSectionOpen", () => {
  it("uses the default when storage is empty", () => {
    expect(parseSectionOpen(null, true)).toBe(true);
    expect(parseSectionOpen(null, false)).toBe(false);
  });

  it("reads 1/0 and falls back for unknown values", () => {
    expect(parseSectionOpen("1")).toBe(true);
    expect(parseSectionOpen("0")).toBe(false);
    expect(parseSectionOpen("yes", true)).toBe(true);
    expect(parseSectionOpen("yes", false)).toBe(false);
  });
});

describe("readSectionOpen / writeSectionOpen", () => {
  beforeEach(() => {
    installMemoryStorage();
  });

  afterEach(() => {
    localStorage.removeItem(sectionStorageKey("trend"));
  });

  it("persists open/closed and defaults when missing", () => {
    expect(readSectionOpen("trend")).toBe(true);
    writeSectionOpen("trend", false);
    expect(readSectionOpen("trend")).toBe(false);
    writeSectionOpen("trend", true);
    expect(readSectionOpen("trend")).toBe(true);
  });
});
