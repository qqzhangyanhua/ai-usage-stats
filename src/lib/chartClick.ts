export function chartClickDataIndex(params: unknown): number | null {
  if (typeof params !== "object" || params === null || !("dataIndex" in params)) {
    return null;
  }
  const index = params.dataIndex;
  return typeof index === "number" && Number.isInteger(index) && index >= 0 ? index : null;
}

export function chartClickName(params: unknown): string | null {
  if (typeof params !== "object" || params === null || !("name" in params)) {
    return null;
  }
  return typeof params.name === "string" && params.name.length > 0 ? params.name : null;
}
