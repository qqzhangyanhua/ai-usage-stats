import type { CursorSessionDetailDto } from "../types";

const cache = new Map<string, CursorSessionDetailDto>();

export function getCachedCursorSessionDetail(
  sourceFile: string,
): CursorSessionDetailDto | undefined {
  return cache.get(sourceFile);
}

export function setCachedCursorSessionDetail(
  sourceFile: string,
  detail: CursorSessionDetailDto,
): void {
  cache.set(sourceFile, detail);
}

export function clearCursorSessionDetailCache(): void {
  cache.clear();
}
