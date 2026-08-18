import { projectLabel } from "./format";
import type {
  ApplicationAnalyticsDto,
  BillingWindowDto,
  CodeVolumeSummary,
  CursorAccountUsageDto,
  CursorSessionSummaryDto,
  OverviewDto,
  WeeklyWindowDto,
} from "../types";

export type ExportTable = {
  headers: string[];
  rows: (string | number)[][];
};

function costCell(cost: number | null): string | number {
  return cost ?? "";
}

function ratioCell(value: number | null): string | number {
  return value ?? "";
}

export function overviewKpiTable(data: OverviewDto, dailyAvg: number): ExportTable {
  return {
    headers: ["指标", "数值", "未定价"],
    rows: [
      ["总 Token", data.total_tokens, ""],
      ["输入 Token", data.input_tokens, ""],
      ["输出 Token", data.output_tokens, ""],
      ["缓存读 Token", data.cache_read_tokens, ""],
      ["缓存写 Token", data.cache_creation_tokens, ""],
      ["推理 Token", data.reasoning_tokens, ""],
      ["会话数", data.session_count, ""],
      ["费用", costCell(data.cost), data.unpriced ? "是" : ""],
      ["日均 Token", Math.round(dailyAvg), ""],
    ],
  };
}

export function billingWindowTable(windows: BillingWindowDto[]): ExportTable {
  return {
    headers: [
      "应用",
      "开始",
      "结束",
      "进行中",
      "Token",
      "会话数",
      "费用",
      "每分钟 Token",
      "每小时费用",
      "预计 Token",
      "预计费用",
    ],
    rows: windows.map((window) => [
      window.application,
      window.start,
      window.end,
      window.is_active ? "是" : "否",
      window.total_tokens,
      window.session_count,
      costCell(window.cost),
      window.burn?.tokens_per_minute ?? "",
      costCell(window.burn?.cost_per_hour ?? null),
      window.projection?.total_tokens ?? "",
      costCell(window.projection?.cost ?? null),
    ]),
  };
}

export function weeklyWindowTable(windows: WeeklyWindowDto[]): ExportTable {
  return {
    headers: ["应用", "开始", "结束", "Token", "会话数", "费用", "日均 Token", "日均费用"],
    rows: windows.map((window) => [
      window.application,
      window.start,
      window.end,
      window.total_tokens,
      window.session_count,
      costCell(window.cost),
      window.daily_average_tokens,
      costCell(window.daily_average_cost),
    ]),
  };
}

export function applicationEfficiencyTable(data: ApplicationAnalyticsDto): ExportTable {
  return {
    headers: ["应用", "总 Token", "会话数", "平均会话 Token", "缓存命中率", "推理占比"],
    rows: data.by_application.map((row) => [
      row.application,
      row.metrics.total_tokens,
      row.metrics.session_count,
      ratioCell(row.metrics.average_session_tokens),
      ratioCell(row.metrics.cache_hit_rate),
      ratioCell(row.metrics.reasoning_share),
    ]),
  };
}

export function applicationProjectMatrixTable(data: ApplicationAnalyticsDto): ExportTable {
  const headers = [
    "项目",
    ...data.by_application.map((application) => application.application),
    "总计",
  ];
  return {
    headers,
    rows: data.projects.map((row) => [
      projectLabel(row.project),
      ...data.by_application.map((application) => row.values[application.source] ?? 0),
      row.total_tokens,
    ]),
  };
}

export function codeVolumeTable(data: CodeVolumeSummary): ExportTable {
  return {
    headers: ["指标", "数值"],
    rows: [
      ["提交数", data.commit_count],
      ["新增行", data.lines_added],
      ["AI 生成行", data.composer_lines_added],
      ["人工编写行", data.human_lines_added],
      ["AI 占比", ratioCell(data.ai_percentage)],
    ],
  };
}

export function cursorAccountDailyTable(data: CursorAccountUsageDto): ExportTable {
  return {
    headers: ["日期", "总量", "输入", "输出"],
    rows: data.daily.map((point) => [
      point.bucket,
      point.total_tokens,
      point.input_tokens,
      point.output_tokens,
    ]),
  };
}

export function cursorAccountModelTable(data: CursorAccountUsageDto): ExportTable {
  return {
    headers: ["模型", "Token", "占比"],
    rows: data.by_model.map((row) => [row.name, row.total_tokens, row.share]),
  };
}

export function cursorSessionProjectTable(data: CursorSessionSummaryDto): ExportTable {
  return {
    headers: ["项目", "会话数", "轮次数"],
    rows: data.by_project.map((row) => [projectLabel(row.name), row.session_count, row.turn_count]),
  };
}

export function cursorSessionToolTable(data: CursorSessionSummaryDto): ExportTable {
  return {
    headers: ["工具", "调用次数"],
    rows: data.top_tools.map((row) => [row.name, row.call_count]),
  };
}
