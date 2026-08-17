import type { EChartsOption } from "echarts";
import type {
  ApplicationEfficiency,
  ApplicationTrendPoint,
  NamedAmount,
  SeriesPoint,
} from "../types";
import { formatCompact } from "./format";
import type { ResolvedTheme } from "../hooks/useTheme";

export type ChartTheme = ResolvedTheme;

const modelPalette = ["#8b6cff", "#3b82f6", "#22d3ee", "#64748b", "#f59e0b", "#34d399", "#f472b6"];

const palettes: Record<
  ChartTheme,
  {
    input: string;
    output: string;
    axis: string;
    text: string;
    axisLabel: string;
    split: string;
    tooltipBg: string;
    tooltipBorder: string;
    tooltipText: string;
    centerValue: string;
    emptySlice: string;
  }
> = {
  dark: {
    input: "#8b6cff",
    output: "#22d3ee",
    axis: "rgba(148, 163, 184, 0.28)",
    text: "#8b97ab",
    axisLabel: "#c9d4e5",
    split: "rgba(148, 163, 184, 0.08)",
    tooltipBg: "#121a2b",
    tooltipBorder: "rgba(255,255,255,0.08)",
    tooltipText: "#e8eef7",
    centerValue: "#f3f6fb",
    emptySlice: "rgba(148,163,184,0.18)",
  },
  light: {
    input: "#7c5cff",
    output: "#0e7490",
    axis: "rgba(71, 85, 105, 0.25)",
    text: "#64748b",
    axisLabel: "#334155",
    split: "rgba(71, 85, 105, 0.1)",
    tooltipBg: "#ffffff",
    tooltipBorder: "rgba(15,23,42,0.1)",
    tooltipText: "#0f172a",
    centerValue: "#0f172a",
    emptySlice: "rgba(100,116,139,0.18)",
  },
};

export function chartPalette(theme: ChartTheme = "dark") {
  return palettes[theme];
}

function paletteFor(theme: ChartTheme = "dark") {
  return palettes[theme];
}

function tooltipBase(theme: ChartTheme) {
  const p = paletteFor(theme);
  return {
    backgroundColor: p.tooltipBg,
    borderColor: p.tooltipBorder,
    textStyle: { color: p.tooltipText, fontSize: 12 },
  };
}

export function areaTrendOption(points: SeriesPoint[], theme: ChartTheme = "dark"): EChartsOption {
  const p = paletteFor(theme);
  return {
    tooltip: {
      ...tooltipBase(theme),
      trigger: "axis",
    },
    legend: {
      data: ["输入 Token", "输出 Token"],
      top: 0,
      left: 44,
      itemWidth: 10,
      itemHeight: 10,
      textStyle: { color: p.text, fontSize: 11 },
    },
    grid: { left: 8, right: 8, top: 36, bottom: 8, containLabel: true },
    xAxis: {
      type: "category",
      boundaryGap: false,
      data: points.map((point) => formatBucket(point.bucket)),
      axisLine: { lineStyle: { color: p.axis } },
      axisTick: { show: false },
      axisLabel: { color: p.text, fontSize: 11 },
    },
    yAxis: {
      type: "value",
      name: "数量",
      nameTextStyle: { color: p.text, fontSize: 11, padding: [0, 0, 0, 8] },
      axisLine: { show: false },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: p.split } },
      axisLabel: {
        color: p.text,
        fontSize: 11,
        formatter: (v: number) => formatCompact(v),
      },
    },
    series: [
      {
        name: "输入 Token",
        type: "line",
        smooth: 0.35,
        symbol: "circle",
        symbolSize: 7,
        showSymbol: true,
        data: points.map((point) => point.input_tokens),
        lineStyle: { width: 2.4, color: p.input },
        itemStyle: { color: p.input },
        areaStyle: {
          color: {
            type: "linear",
            x: 0,
            y: 0,
            x2: 0,
            y2: 1,
            colorStops: [
              { offset: 0, color: "rgba(139,108,255,0.38)" },
              { offset: 1, color: "rgba(139,108,255,0.02)" },
            ],
          },
        },
      },
      {
        name: "输出 Token",
        type: "line",
        smooth: 0.35,
        symbol: "circle",
        symbolSize: 7,
        showSymbol: true,
        data: points.map((point) => point.output_tokens),
        lineStyle: { width: 2.4, color: p.output },
        itemStyle: { color: p.output },
        areaStyle: {
          color: {
            type: "linear",
            x: 0,
            y: 0,
            x2: 0,
            y2: 1,
            colorStops: [
              { offset: 0, color: "rgba(34,211,238,0.32)" },
              { offset: 1, color: "rgba(34,211,238,0.02)" },
            ],
          },
        },
      },
    ],
  };
}

export function barTrendOption(points: SeriesPoint[], theme: ChartTheme = "dark"): EChartsOption {
  const p = paletteFor(theme);
  return {
    tooltip: { ...tooltipBase(theme), trigger: "axis" },
    grid: { left: 8, right: 8, top: 16, bottom: 8, containLabel: true },
    xAxis: {
      type: "category",
      data: points.map((point) => formatBucket(point.bucket)),
      axisLine: { lineStyle: { color: p.axis } },
      axisTick: { show: false },
      axisLabel: { color: p.text, fontSize: 11 },
    },
    yAxis: {
      type: "value",
      splitLine: { lineStyle: { color: p.split } },
      axisLabel: {
        color: p.text,
        fontSize: 11,
        formatter: (v: number) => formatCompact(v),
      },
    },
    series: [
      {
        type: "bar",
        name: "token",
        data: points.map((point) => point.total_tokens),
        barMaxWidth: 28,
        itemStyle: {
          color: {
            type: "linear",
            x: 0,
            y: 0,
            x2: 0,
            y2: 1,
            colorStops: [
              { offset: 0, color: "#8b6cff" },
              { offset: 1, color: "#3b82f6" },
            ],
          },
          borderRadius: [6, 6, 0, 0],
        },
      },
    ],
  };
}

export function applicationStackedTrendOption(
  points: ApplicationTrendPoint[],
  applications: ApplicationEfficiency[],
  theme: ChartTheme = "dark",
): EChartsOption {
  const p = paletteFor(theme);
  return {
    tooltip: {
      ...tooltipBase(theme),
      trigger: "axis",
      axisPointer: { type: "shadow" },
    },
    legend: {
      type: "scroll",
      data: applications.map((item) => item.application),
      top: 0,
      left: 12,
      right: 12,
      itemWidth: 10,
      itemHeight: 10,
      textStyle: { color: p.text, fontSize: 11 },
      pageTextStyle: { color: p.text },
    },
    grid: { left: 8, right: 18, top: 42, bottom: 8, containLabel: true },
    xAxis: {
      type: "category",
      data: points.map((point) => formatBucket(point.bucket)),
      axisLine: { lineStyle: { color: p.axis } },
      axisTick: { show: false },
      axisLabel: { color: p.text, fontSize: 11 },
    },
    yAxis: {
      type: "value",
      name: "Token",
      nameTextStyle: { color: p.text, fontSize: 11 },
      splitLine: { lineStyle: { color: p.split } },
      axisLabel: {
        color: p.text,
        fontSize: 11,
        formatter: (value: number) => formatCompact(value),
      },
    },
    series: applications.map((application, index) => ({
      name: application.application,
      type: "bar",
      stack: "applications",
      barMaxWidth: 38,
      emphasis: { focus: "series" },
      itemStyle: {
        color: modelPalette[index % modelPalette.length],
        borderRadius: index === applications.length - 1 ? [5, 5, 0, 0] : 0,
      },
      data: points.map((point) => point.values[application.source] ?? 0),
    })),
  };
}

export function breakdownBarOption(
  labels: string[],
  values: number[],
  theme: ChartTheme = "dark",
): EChartsOption {
  const p = paletteFor(theme);
  return {
    tooltip: { ...tooltipBase(theme), trigger: "axis", axisPointer: { type: "shadow" } },
    grid: { left: 8, right: 24, top: 8, bottom: 8, containLabel: true },
    xAxis: {
      type: "value",
      splitLine: { lineStyle: { color: p.split } },
      axisLabel: {
        color: p.text,
        fontSize: 11,
        formatter: (v: number) => formatCompact(v),
      },
    },
    yAxis: {
      type: "category",
      data: labels,
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: { color: p.axisLabel, fontSize: 12 },
    },
    series: [
      {
        type: "bar",
        data: values,
        barMaxWidth: 16,
        itemStyle: {
          color: {
            type: "linear",
            x: 0,
            y: 0,
            x2: 1,
            y2: 0,
            colorStops: [
              { offset: 0, color: "#5b4dff" },
              { offset: 1, color: "#22d3ee" },
            ],
          },
          borderRadius: [0, 8, 8, 0],
        },
      },
    ],
  };
}

export function donutOption(
  items: { name: string; value: number; color: string }[],
  centerValue: string,
  theme: ChartTheme = "dark",
): EChartsOption {
  const p = paletteFor(theme);
  const hasData = items.some((item) => item.value > 0);
  const slices = hasData
    ? items.filter((item) => item.value > 0)
    : [{ name: "暂无", value: 1, color: p.emptySlice }];
  return {
    tooltip: hasData
      ? {
          ...tooltipBase(theme),
          trigger: "item",
          formatter: (raw: unknown) => {
            const item = raw as { name: string; value: number; percent: number };
            return `${item.name}<br/>${formatCompact(item.value)} (${item.percent.toFixed(1)}%)`;
          },
        }
      : { show: false },
    series: [
      {
        type: "pie",
        radius: ["62%", "84%"],
        center: ["50%", "50%"],
        avoidLabelOverlap: false,
        label: { show: false },
        labelLine: { show: false },
        silent: !hasData,
        data: slices.map((item) => ({
          name: item.name,
          value: item.value,
          itemStyle: { color: item.color, borderWidth: 0 },
        })),
      },
    ],
    graphic: [
      {
        type: "text",
        left: "center",
        top: "42%",
        style: {
          text: "总计",
          fill: p.text,
          fontSize: 11,
          align: "center",
        },
      },
      {
        type: "text",
        left: "center",
        top: "52%",
        style: {
          text: centerValue,
          fill: p.centerValue,
          fontSize: 18,
          fontWeight: 650,
          align: "center",
        },
      },
    ],
  };
}

export function modelSlices(rows: NamedAmount[]): { name: string; value: number; color: string }[] {
  const top = rows.slice(0, 3);
  const rest = rows.slice(3);
  const items = top.map((row, i) => ({
    name: row.name,
    value: row.total_tokens,
    color: modelPalette[i] ?? "#64748b",
  }));
  const restTotal = rest.reduce((sum, row) => sum + row.total_tokens, 0);
  if (restTotal > 0) {
    items.push({ name: "其他", value: restTotal, color: modelPalette[3] ?? "#64748b" });
  }
  return items;
}

export function formatBucket(bucket: string): string {
  if (/^\d{4}-\d{2}-\d{2}$/.test(bucket)) {
    return bucket.slice(5);
  }
  return bucket;
}
