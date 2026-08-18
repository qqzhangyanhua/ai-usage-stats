import { overviewKpiTable } from "../lib/exportRows";
import type { OverviewDto } from "../types";
import { ExportButton } from "./ExportButton";

export function OverviewExport({
  data,
  dailyAvg,
}: {
  data: OverviewDto;
  dailyAvg: number;
}) {
  const table = overviewKpiTable(data, dailyAvg);
  return <ExportButton filename="总览KPI" headers={table.headers} rows={table.rows} />;
}
