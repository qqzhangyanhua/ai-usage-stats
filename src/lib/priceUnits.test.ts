import { describe, expect, it } from "vitest";
import {
  formatPerMillionInput,
  fromPerMillion,
  parsePerMillionInput,
  toPerMillion,
} from "./priceUnits";

describe("priceUnits", () => {
  it("round-trips common USD/1M prices back to per-token storage", () => {
    expect(fromPerMillion(3)).toBeCloseTo(0.000003, 12);
    expect(toPerMillion(0.000003)).toBeCloseTo(3, 12);
    expect(fromPerMillion(toPerMillion(0.0000001))).toBeCloseTo(0.0000001, 16);
  });

  it("formats tiny per-token values without float noise in the input box", () => {
    expect(formatPerMillionInput(0)).toBe("0");
    expect(formatPerMillionInput(Number.NaN)).toBe("0");
    expect(formatPerMillionInput(5 / 1_000_000)).toBe("5");
    expect(formatPerMillionInput(0.22 / 1_000_000)).toBe("0.22");
  });

  it("rejects invalid USD/1M edits instead of writing NaN", () => {
    expect(parsePerMillionInput("")).toBe(0);
    expect(parsePerMillionInput("3")).toBeCloseTo(0.000003, 12);
    expect(parsePerMillionInput("abc")).toBeNull();
    expect(parsePerMillionInput("-1")).toBeNull();
  });
});
