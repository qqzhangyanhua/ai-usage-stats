import { useState } from "react";
import { exportCsv, exportJson } from "../lib/exportFile";
import { Button } from "./ui/Button";

type ExportFormat = "csv" | "json";
type ExportRows = (string | number)[][];

export function ExportButton({
  label = "导出",
  filename,
  headers,
  rows,
  getRows,
  disabled: disabledProp,
}: {
  label?: string;
  /** 不带扩展名的文件基础名，例如「会话列表」 */
  filename: string;
  headers: string[];
  /** 静态数据（已经在内存中的全部行）。与 `getRows` 二选一。 */
  rows?: ExportRows;
  /** 懒加载数据源，用于服务端分页场景：点击时才拉取完整结果集。 */
  getRows?: () => Promise<ExportRows>;
  disabled?: boolean;
}) {
  const [status, setStatus] = useState<string | null>(null);
  const [busyFormat, setBusyFormat] = useState<ExportFormat | null>(null);

  async function handleExport(format: ExportFormat) {
    setBusyFormat(format);
    setStatus(null);
    try {
      const data = getRows ? await getRows() : (rows ?? []);
      if (data.length === 0) {
        setStatus("没有可导出的数据");
        return;
      }
      const saved =
        format === "csv"
          ? await exportCsv(`${filename}.csv`, headers, data)
          : await exportJson(`${filename}.json`, headers, data);
      setStatus(saved ? "已导出" : null);
    } catch (error) {
      setStatus(error instanceof Error ? error.message : "导出失败");
    } finally {
      setBusyFormat(null);
    }
  }

  const staticEmpty = !getRows && (rows?.length ?? 0) === 0;
  const disabled = (disabledProp ?? staticEmpty) || busyFormat !== null;

  return (
    <span className="export-action">
      <span className="export-group" role="group" aria-label={`${label}选项`}>
        <Button disabled={disabled} onClick={() => handleExport("csv")} aria-label={`${label} CSV`}>
          {busyFormat === "csv" ? "导出中…" : `${label} CSV`}
        </Button>
        <Button
          disabled={disabled}
          onClick={() => handleExport("json")}
          aria-label={`${label} JSON`}
        >
          {busyFormat === "json" ? "导出中…" : `${label} JSON`}
        </Button>
      </span>
      {status ? <span className="export-status">{status}</span> : null}
    </span>
  );
}
