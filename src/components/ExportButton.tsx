import { useState } from "react";
import { Icon } from "../icons";
import { exportCsv, exportJson } from "../lib/exportFile";
import { Spinner } from "./Spinner";

type ExportFormat = "csv" | "json";
type ExportRows = (string | number)[][];

const FORMATS: { format: ExportFormat; label: string }[] = [
  { format: "csv", label: "CSV" },
  { format: "json", label: "JSON" },
];

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
        <span className="export-group-icon" aria-hidden>
          {busyFormat ? <Spinner size={12} /> : <Icon name="download" size={13} />}
        </span>
        {FORMATS.map(({ format, label: formatLabel }) => (
          <button
            key={format}
            type="button"
            className={`export-btn export-btn-${format}${busyFormat === format ? " is-busy" : ""}`}
            disabled={disabled}
            aria-label={`${label} ${formatLabel}`}
            aria-busy={busyFormat === format}
            onClick={() => handleExport(format)}
          >
            {formatLabel}
          </button>
        ))}
      </span>
      {status ? <span className="export-status">{status}</span> : null}
    </span>
  );
}
