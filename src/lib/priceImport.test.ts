import { describe, expect, it } from "vitest";
import { mergePricePresets } from "./priceImport";
import type { PricePreset } from "./pricePresets";

const samplePreset: PricePreset = {
  id: "test-model",
  providerLabel: "Test",
  model: "test-model",
  displayName: "Test Model",
  inputPerM: 1,
  outputPerM: 2,
  cacheReadPerM: 0.1,
  cacheWritePerM: 0.2,
  asOf: "2026-08-18",
};

describe("mergePricePresets", () => {
  it("adds new entries and skips duplicates", () => {
    const current = [
      {
        model: "existing",
        provider: null,
        input: 0,
        output: 0,
        cache_read: 0,
        cache_creation: 0,
      },
    ];
    const result = mergePricePresets(current, [samplePreset, samplePreset], ["test-model"]);
    expect(result.additions).toHaveLength(1);
    expect(result.additions[0]?.model).toBe("test-model");
    expect(result.skipped).toBe(1);
    expect(result.message).toContain("已添加 1 条");
  });

  it("reports when everything already exists", () => {
    const current = [
      {
        model: "test-model",
        provider: null,
        input: 0,
        output: 0,
        cache_read: 0,
        cache_creation: 0,
      },
    ];
    const result = mergePricePresets(current, [samplePreset], []);
    expect(result.additions).toHaveLength(0);
    expect(result.skipped).toBe(1);
    expect(result.message).toContain("未添加新条目");
  });
});
