import { projectLabel } from "./format";
import type {
  ApplicationAnalyticsDto,
  CodeVolumeSummary,
  CursorAccountUsageDto,
  CursorSessionSummaryDto,
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
  const unpriced = data.cost_unpriced ? "是" : "";
  return {
    headers: ["指标", "数值", "未定价"],
    rows: [
      ["提交数", data.commit_count, ""],
      ["新增行", data.lines_added, ""],
      ["AI 生成行", data.composer_lines_added, ""],
      ["人工编写行", data.human_lines_added, ""],
      ["AI 占比", ratioCell(data.ai_percentage), ""],
      ["全部来源累计费用", costCell(data.total_cost), unpriced],
      ["每千行 AI 代码成本", costCell(data.cost_per_thousand_ai_lines), unpriced],
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
