import type { PriceEntry } from "../types";
import { matchObservedModel, presetToPriceEntry, type PricePreset } from "./pricePresets";

export type PriceImportResult = {
  additions: PriceEntry[];
  skipped: number;
  message: string;
};

export function mergePricePresets(
  current: PriceEntry[],
  chosen: PricePreset[],
  observedModels: string[],
): PriceImportResult {
  if (chosen.length === 0) {
    return { additions: [], skipped: 0, message: "" };
  }
  const existing = new Set(current.map((row) => `${row.model}::${row.provider ?? ""}`));
  const additions: PriceEntry[] = [];
  let skipped = 0;
  for (const preset of chosen) {
    const matched = matchObservedModel(preset, observedModels);
    const entry = presetToPriceEntry(preset, matched ?? undefined);
    const key = `${entry.model}::${entry.provider ?? ""}`;
    if (existing.has(key)) {
      skipped += 1;
      continue;
    }
    existing.add(key);
    additions.push(entry);
  }
  if (additions.length > 0) {
    return {
      additions,
      skipped,
      message: `已添加 ${additions.length} 条${skipped > 0 ? `，跳过 ${skipped} 条已存在` : ""}，别忘了点保存`,
    };
  }
  return {
    additions: [],
    skipped,
    message: `未添加新条目（${skipped} 条已存在于当前单价表）`,
  };
}
