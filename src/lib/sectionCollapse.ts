export const SECTION_STORAGE_PREFIX = "mabiao:overview-section:";

export function sectionStorageKey(sectionId: string): string {
  return `${SECTION_STORAGE_PREFIX}${sectionId}`;
}

export function parseSectionOpen(raw: string | null, defaultOpen = true): boolean {
  if (raw === null) {
    return defaultOpen;
  }
  if (raw === "1") {
    return true;
  }
  if (raw === "0") {
    return false;
  }
  return defaultOpen;
}

export function readSectionOpen(sectionId: string, defaultOpen = true): boolean {
  try {
    return parseSectionOpen(localStorage.getItem(sectionStorageKey(sectionId)), defaultOpen);
  } catch {
    return defaultOpen;
  }
}

export function writeSectionOpen(sectionId: string, open: boolean): void {
  try {
    localStorage.setItem(sectionStorageKey(sectionId), open ? "1" : "0");
  } catch {
    /* quota / private mode */
  }
}
