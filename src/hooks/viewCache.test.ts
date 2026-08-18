import { describe, expect, it } from "vitest";
import type { Filter, View } from "../types";
import { isViewFresh, parseViewHash, viewStamp, viewsWarmedBy } from "./viewCache";

const filter: Filter = {
  from: null,
  to: null,
  sources: [],
  models: [],
  projects: [],
  providers: [],
};

describe("parseViewHash", () => {
  it("maps known view hashes", () => {
    expect(parseViewHash("#sessions")).toBe("sessions");
    expect(parseViewHash("source")).toBe("application");
  });

  it("keeps settings panel anchors on the settings view", () => {
    expect(parseViewHash("#settings")).toBe("settings");
    expect(parseViewHash("#settings-budget")).toBe("settings");
    expect(parseViewHash("settings-diagnostics")).toBe("settings");
  });

  it("falls back to overview for unknown hashes", () => {
    expect(parseViewHash("#nope")).toBe("overview");
  });
});

describe("viewStamp", () => {
  it("keeps model/provider/project stamps stable across grain changes", () => {
    const day = viewStamp("model", filter, "all", "day", 1);
    const week = viewStamp("model", filter, "all", "week", 1);
    expect(day).toBe(week);
    expect(viewStamp("trend", filter, "all", "day", 1)).not.toBe(
      viewStamp("trend", filter, "all", "week", 1),
    );
  });

  it("invalidates every view when ingest epoch bumps", () => {
    const before = viewStamp("trend", filter, "all", "day", 1);
    const after = viewStamp("trend", filter, "all", "day", 2);
    expect(before).not.toBe(after);
  });
});

describe("viewsWarmedBy", () => {
  it("marks trend/model/project warm after overview", () => {
    expect(viewsWarmedBy("overview")).toEqual(["overview", "trend", "model", "project"]);
    expect(viewsWarmedBy("trend")).toEqual(["trend"]);
  });
});

describe("isViewFresh", () => {
  it("hits after overview warm and misses after filter change", () => {
    const loaded: Partial<Record<View, string>> = {};
    for (const view of viewsWarmedBy("overview")) {
      loaded[view] = viewStamp(view, filter, "all", "day", 1);
    }
    expect(isViewFresh(loaded, "trend", filter, "all", "day", 1)).toBe(true);
    expect(isViewFresh(loaded, "model", filter, "all", "week", 1)).toBe(true);
    expect(isViewFresh(loaded, "trend", { ...filter, from: "2026-08-01" }, "all", "day", 1)).toBe(
      false,
    );
  });
});
