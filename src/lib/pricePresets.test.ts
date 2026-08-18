import { describe, expect, it } from "vitest";
import {
  PRICE_PRESETS,
  groupPresetsByProvider,
  matchObservedModel,
  presetToPriceEntry,
  type PricePreset,
} from "./pricePresets";

describe("matchObservedModel", () => {
  const preset = PRICE_PRESETS.find((p) => p.model === "claude-sonnet-5") as PricePreset;

  it("finds an exact match ignoring case and separators", () => {
    expect(matchObservedModel(preset, ["Claude-Sonnet-5"])).toBe("Claude-Sonnet-5");
  });

  it("falls back to a substring match when no exact match exists", () => {
    expect(matchObservedModel(preset, ["claude-sonnet-5-20260801"])).toBe(
      "claude-sonnet-5-20260801",
    );
  });

  it("returns null when nothing resembles the preset", () => {
    expect(matchObservedModel(preset, ["gpt-5.6-sol", "gemini-3.1-pro-preview"])).toBeNull();
  });
});

describe("presetToPriceEntry", () => {
  it("converts per-million prices to per-token prices", () => {
    const preset: PricePreset = {
      id: "test",
      providerLabel: "Test",
      model: "test-model",
      displayName: "Test Model",
      inputPerM: 2,
      outputPerM: 10,
      cacheReadPerM: 0.2,
      cacheWritePerM: 2.5,
      asOf: "2026-08-16",
    };
    const entry = presetToPriceEntry(preset);
    expect(entry.model).toBe("test-model");
    expect(entry.provider).toBeNull();
    expect(entry.input).toBeCloseTo(0.000002);
    expect(entry.output).toBeCloseTo(0.00001);
    expect(entry.cache_read).toBeCloseTo(0.0000002);
    expect(entry.cache_creation).toBeCloseTo(0.0000025);
  });

  it("overrides the model name when a local match is provided", () => {
    const preset = PRICE_PRESETS[0];
    const entry = presetToPriceEntry(preset, "observed-model-name");
    expect(entry.model).toBe("observed-model-name");
  });
});

describe("groupPresetsByProvider", () => {
  it("groups all presets under their provider label, preserving order", () => {
    const groups = groupPresetsByProvider(PRICE_PRESETS);
    const providerLabels = groups.map(([label]) => label);
    expect(new Set(providerLabels).size).toBe(providerLabels.length);
    const total = groups.reduce((sum, [, presets]) => sum + presets.length, 0);
    expect(total).toBe(PRICE_PRESETS.length);
  });

  it("keeps every preset in a group matching its own providerLabel", () => {
    const groups = groupPresetsByProvider(PRICE_PRESETS);
    for (const [label, presets] of groups) {
      for (const preset of presets) {
        expect(preset.providerLabel).toBe(label);
      }
    }
  });
});
