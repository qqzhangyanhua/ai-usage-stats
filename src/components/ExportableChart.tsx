import type { EChartsOption } from "echarts";
import ReactECharts from "echarts-for-react";
import { useRef, useState, type CSSProperties } from "react";
import { Icon } from "../icons";
import { exportImage } from "../lib/exportFile";

/**
 * 包裹 ReactECharts，在图表右上角提供「导出为图片」的悬浮按钮。
 */
export function ExportableChart({
  option,
  style,
  filename,
}: {
  option: EChartsOption;
  style?: CSSProperties;
  filename: string;
}) {
  const chartRef = useRef<ReactECharts | null>(null);
  const [busy, setBusy] = useState(false);

  async function handleExport() {
    const instance = chartRef.current?.getEchartsInstance();
    if (!instance) {
      return;
    }
    setBusy(true);
    try {
      const dataUrl = instance.getDataURL({
        type: "png",
        pixelRatio: 2,
        backgroundColor: "transparent",
      });
      await exportImage(`${filename}.png`, dataUrl);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="chart-frame">
      <button
        type="button"
        className="chart-export-btn"
        onClick={handleExport}
        disabled={busy}
        aria-label="导出图表为图片"
        title="导出图表为图片"
      >
        <Icon name="download" size={13} />
      </button>
      <ReactECharts ref={chartRef} option={option} style={style} />
    </div>
  );
}
