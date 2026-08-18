import { describe, expect, it } from "vitest";
import { chartClickDataIndex, chartClickName } from "./chartClick";

describe("chartClickDataIndex", () => {
  it("reads a non-negative integer dataIndex", () => {
    expect(chartClickDataIndex({ dataIndex: 2 })).toBe(2);
    expect(chartClickDataIndex({ dataIndex: 0 })).toBe(0);
  });

  it("rejects missing or invalid indexes", () => {
    expect(chartClickDataIndex(null)).toBeNull();
    expect(chartClickDataIndex({})).toBeNull();
    expect(chartClickDataIndex({ dataIndex: -1 })).toBeNull();
    expect(chartClickDataIndex({ dataIndex: 1.5 })).toBeNull();
  });
});

describe("chartClickName", () => {
  it("reads a non-empty name", () => {
    expect(chartClickName({ name: "gpt-5" })).toBe("gpt-5");
  });

  it("rejects missing or empty names", () => {
    expect(chartClickName(null)).toBeNull();
    expect(chartClickName({ name: "" })).toBeNull();
    expect(chartClickName({})).toBeNull();
  });
});
