import ReactECharts from "echarts-for-react";
import type { EChartsOption } from "echarts";

export function DonutChart({
  option,
  centerLabel = "总计",
  centerValue,
}: {
  option: EChartsOption;
  centerLabel?: string;
  centerValue: string;
}) {
  const valueSize =
    centerValue.length >= 10 ? "is-long" : centerValue.length >= 8 ? "is-medium" : "";

  return (
    <div className="donut-chart">
      <ReactECharts option={option} style={{ height: "100%", width: "100%" }} />
      <div className="donut-center">
        <span>{centerLabel}</span>
        <strong className={valueSize} title={centerValue}>
          {centerValue}
        </strong>
      </div>
    </div>
  );
}
