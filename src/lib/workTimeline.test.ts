import { describe, expect, it } from "vitest";
import type { WorkSegment } from "../types";
import {
  DAY_MINUTES,
  dayStartIso,
  laneCount,
  layoutSegments,
  minutesSinceDayStart,
  MIN_SEGMENT_MINUTES,
} from "./workTimeline";

const DAY_START = "2026-08-15T00:00:00.000Z";

function seg(overrides: Partial<WorkSegment> = {}): WorkSegment {
  return {
    session_id: "s1",
    source: "codex",
    project: "/proj/a",
    model: "gpt-5.1-codex",
    start: "2026-08-15T01:00:00.000Z",
    end: "2026-08-15T02:00:00.000Z",
    total_tokens: 100,
    ...overrides,
  };
}

describe("minutesSinceDayStart", () => {
  it("computes minutes from an epoch difference", () => {
    expect(minutesSinceDayStart("2026-08-15T01:30:00.000Z", DAY_START)).toBe(90);
    expect(minutesSinceDayStart(DAY_START, DAY_START)).toBe(0);
  });

  it("clamps to [0, DAY_MINUTES]", () => {
    expect(minutesSinceDayStart("2026-08-14T23:00:00.000Z", DAY_START)).toBe(0);
    expect(minutesSinceDayStart("2026-08-16T05:00:00.000Z", DAY_START)).toBe(DAY_MINUTES);
  });

  it("returns 0 for unparsable input instead of throwing", () => {
    expect(minutesSinceDayStart("not-a-date", DAY_START)).toBe(0);
  });
});

describe("layoutSegments", () => {
  it("keeps non-overlapping segments on a single lane", () => {
    const segments = [
      seg({ session_id: "a", start: "2026-08-15T01:00:00.000Z", end: "2026-08-15T02:00:00.000Z" }),
      seg({ session_id: "b", start: "2026-08-15T02:00:00.000Z", end: "2026-08-15T03:00:00.000Z" }),
      seg({ session_id: "c", start: "2026-08-15T03:30:00.000Z", end: "2026-08-15T04:00:00.000Z" }),
    ];
    const layout = layoutSegments(segments, DAY_START);
    expect(layout.every((item) => item.lane === 0)).toBe(true);
    expect(laneCount(layout)).toBe(1);
  });

  it("stacks overlapping segments onto separate lanes", () => {
    const segments = [
      seg({ session_id: "a", start: "2026-08-15T01:00:00.000Z", end: "2026-08-15T03:00:00.000Z" }),
      seg({ session_id: "b", start: "2026-08-15T01:30:00.000Z", end: "2026-08-15T02:30:00.000Z" }),
    ];
    const layout = layoutSegments(segments, DAY_START);
    const byId = new Map(layout.map((item) => [item.segment.session_id, item.lane]));
    expect(byId.get("a")).toBe(0);
    expect(byId.get("b")).toBe(1);
    expect(laneCount(layout)).toBe(2);
  });

  it("assigns a distinct lane per session when three overlap at once", () => {
    const segments = [
      seg({ session_id: "a", start: "2026-08-15T01:00:00.000Z", end: "2026-08-15T04:00:00.000Z" }),
      seg({ session_id: "b", start: "2026-08-15T01:30:00.000Z", end: "2026-08-15T03:30:00.000Z" }),
      seg({ session_id: "c", start: "2026-08-15T02:00:00.000Z", end: "2026-08-15T03:00:00.000Z" }),
    ];
    const layout = layoutSegments(segments, DAY_START);
    expect(laneCount(layout)).toBe(3);
  });

  it("reuses a freed lane once its segment has ended", () => {
    const segments = [
      seg({ session_id: "a", start: "2026-08-15T01:00:00.000Z", end: "2026-08-15T02:00:00.000Z" }),
      seg({ session_id: "b", start: "2026-08-15T01:15:00.000Z", end: "2026-08-15T01:45:00.000Z" }),
      seg({ session_id: "c", start: "2026-08-15T02:30:00.000Z", end: "2026-08-15T03:00:00.000Z" }),
    ];
    const layout = layoutSegments(segments, DAY_START);
    const byId = new Map(layout.map((item) => [item.segment.session_id, item.lane]));
    expect(byId.get("a")).toBe(0);
    expect(byId.get("b")).toBe(1);
    // c starts after both a and b have ended, so it should reuse lane 0.
    expect(byId.get("c")).toBe(0);
    expect(laneCount(layout)).toBe(2);
  });

  it("gives zero-width segments a minimum visible width without mutating the source segment", () => {
    const zeroWidth = seg({
      session_id: "z",
      start: "2026-08-15T05:00:00.000Z",
      end: "2026-08-15T05:00:00.000Z",
    });
    const [placed] = layoutSegments([zeroWidth], DAY_START);
    expect(placed.endMinutes - placed.startMinutes).toBe(MIN_SEGMENT_MINUTES);
    expect(placed.segment).toBe(zeroWidth);
    expect(zeroWidth.start).toBe(zeroWidth.end);
  });

  it("is insensitive to input order", () => {
    const early = seg({ session_id: "early", start: "2026-08-15T01:00:00.000Z", end: "2026-08-15T01:30:00.000Z" });
    const late = seg({ session_id: "late", start: "2026-08-15T05:00:00.000Z", end: "2026-08-15T05:30:00.000Z" });
    const forward = layoutSegments([early, late], DAY_START);
    const backward = layoutSegments([late, early], DAY_START);
    const laneOf = (layout: typeof forward, id: string) =>
      layout.find((item) => item.segment.session_id === id)?.lane;
    expect(laneOf(forward, "early")).toBe(laneOf(backward, "early"));
    expect(laneOf(forward, "late")).toBe(laneOf(backward, "late"));
  });
});

describe("laneCount", () => {
  it("returns 0 for an empty layout", () => {
    expect(laneCount([])).toBe(0);
  });
});

describe("dayStartIso", () => {
  it("round-trips with minutesSinceDayStart to give a zero offset for the same day", () => {
    const start = dayStartIso("2026-08-15");
    expect(minutesSinceDayStart(start, start)).toBe(0);
    expect(Number.isNaN(Date.parse(start))).toBe(false);
  });
});
