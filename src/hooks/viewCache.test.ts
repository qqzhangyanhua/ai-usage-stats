import { describe, expect, it } from "vitest";
import type { Filter, View } from "../types";
import {
  emptyViewScope,
  filtersEqual,
  initialViewScopes,
  isViewFresh,
  reconcileLoadedStamps,
  viewStamp,
  viewsInvalidatedBy,
  viewsWarmedBy,
} from "./viewCache";

const filter: Filter = {
  from: null,
  to: null,
  sources: [],
  models: [],
  projects: [],
  providers: [],
};

const ranged: Filter = { ...filter, from: "2026-08-01", to: "2026-08-07" };

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

describe("filtersEqual", () => {
  it("treats the same membership as equal regardless of order", () => {
    expect(
      filtersEqual({ ...filter, projects: ["a", "b"] }, { ...filter, projects: ["b", "a"] }),
    ).toBe(true);
    expect(filtersEqual(filter, ranged)).toBe(false);
  });
});

describe("viewsWarmedBy", () => {
  it("marks trend/model/project warm after overview", () => {
    expect(viewsWarmedBy("overview")).toEqual(["overview", "trend", "model", "project"]);
    expect(viewsWarmedBy("trend")).toEqual(["trend"]);
  });
});

describe("viewsInvalidatedBy", () => {
  it("invalidates shared datasets written by the current view", () => {
    expect(viewsInvalidatedBy("overview")).toEqual(["trend", "model", "project"]);
    expect(viewsInvalidatedBy("sessions")).toEqual([]);
    expect(viewsInvalidatedBy("trend")).toEqual(["overview"]);
  });
});

describe("reconcileLoadedStamps", () => {
  it("warms sibling views only when their filters still match", () => {
    const scopes = initialViewScopes();
    const used = emptyViewScope();
    const loaded = reconcileLoadedStamps({}, "overview", used, scopes, "day", 1);

    expect(loaded.overview).toBe(viewStamp("overview", used.filter, used.preset, "day", 1));
    expect(loaded.trend).toBe(viewStamp("trend", used.filter, used.preset, "day", 1));
    expect(loaded.project).toBe(viewStamp("project", used.filter, used.preset, "day", 1));
  });

  it("does not leak one view's filter into another view's cache stamp", () => {
    const scopes = initialViewScopes();
    scopes.project = { filter: ranged, preset: "7" };
    const used = emptyViewScope();
    const loaded = reconcileLoadedStamps(
      {
        project: viewStamp("project", ranged, "7", "day", 1),
      },
      "overview",
      used,
      scopes,
      "day",
      1,
    );

    expect(loaded.overview).toBe(viewStamp("overview", used.filter, used.preset, "day", 1));
    expect(loaded.project).toBeUndefined();
    expect(isViewFresh(loaded, "project", ranged, "7", "day", 1)).toBe(false);
  });

  it("invalidates overview when a sibling overwrites shared data with a different filter", () => {
    const scopes = initialViewScopes();
    const overviewScope = emptyViewScope();
    const projectScope = { filter: ranged, preset: "7" };
    scopes.project = projectScope;
    const afterOverview = reconcileLoadedStamps({}, "overview", overviewScope, scopes, "day", 1);
    const afterProject = reconcileLoadedStamps(
      afterOverview,
      "project",
      projectScope,
      scopes,
      "day",
      1,
    );

    expect(afterProject.project).toBe(viewStamp("project", ranged, "7", "day", 1));
    expect(afterProject.overview).toBeUndefined();
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
    expect(
      isViewFresh(loaded, "trend", { ...filter, from: "2026-08-01" }, "all", "day", 1),
    ).toBe(false);
  });
});
