import { describe, expect, it } from "vitest";
import {
  clearDimensionFilters,
  filterChips,
  hasDimensionFilters,
  removeFilterChip,
} from "./filterChips";
import type { Filter } from "../types";

const filter: Filter = {
  from: null,
  to: null,
  sources: ["claude"],
  models: ["gpt-5"],
  projects: ["/proj/a"],
  providers: ["anthropic"],
};

describe("filterChips", () => {
  it("lists every selected dimension as a chip", () => {
    expect(filterChips(filter).map((chip) => chip.id)).toEqual([
      "project:/proj/a",
      "source:claude",
      "model:gpt-5",
      "provider:anthropic",
    ]);
  });

  it("clears only dimension filters", () => {
    const next = clearDimensionFilters({ ...filter, from: "2026-08-01" });
    expect(next.from).toBe("2026-08-01");
    expect(hasDimensionFilters(next)).toBe(false);
  });

  it("removes a single chip", () => {
    const next = removeFilterChip(filter, { id: "source:claude", kind: "source", value: "claude" });
    expect(next.sources).toEqual([]);
    expect(next.models).toEqual(["gpt-5"]);
  });
});
