import { describe, expect, it } from "vitest";
import {
  applicationEfficiencyTable,
  applicationProjectMatrixTable,
  billingWindowTable,
  codeVolumeTable,
  cursorAccountDailyTable,
  cursorAccountModelTable,
  cursorSessionProjectTable,
  cursorSessionToolTable,
  weeklyWindowTable,
} from "./exportRows";
import type {
  ApplicationAnalyticsDto,
  BillingWindowDto,
  CodeVolumeSummary,
  CursorAccountUsageDto,
  CursorSessionSummaryDto,
  WeeklyWindowDto,
} from "../types";

describe("billingWindowTable", () => {
  it("maps window, burn, and projection fields", () => {
    const window: BillingWindowDto = {
      source: "claude",
      application: "Claude Code",
      start: "2026-08-18T00:00:00Z",
      end: "2026-08-18T05:00:00Z",
      last_activity: "2026-08-18T01:00:00Z",
      is_active: true,
      elapsed_minutes: 60,
      remaining_minutes: 240,
      total_tokens: 500,
      input_tokens: 400,
      output_tokens: 100,
      cache_read_tokens: 0,
      cache_creation_tokens: 0,
      reasoning_tokens: 0,
      session_count: 2,
      cost: 0.4,
      unpriced: false,
      burn: { tokens_per_minute: 8.3, cost_per_hour: 0.2 },
      projection: { total_tokens: 2500, cost: 2 },
    };
    expect(billingWindowTable([window]).rows[0]).toEqual([
      "Claude Code",
      "2026-08-18T00:00:00Z",
      "2026-08-18T05:00:00Z",
      "是",
      500,
      2,
      0.4,
      8.3,
      0.2,
      2500,
      2,
    ]);
  });
});

describe("weeklyWindowTable", () => {
  it("exports daily averages and blank unpriced cost", () => {
    const window: WeeklyWindowDto = {
      source: "codex",
      application: "Codex",
      window_days: 7,
      start: "2026-08-11T00:00:00Z",
      end: "2026-08-18T00:00:00Z",
      total_tokens: 700,
      input_tokens: 500,
      output_tokens: 200,
      cache_read_tokens: 0,
      cache_creation_tokens: 0,
      reasoning_tokens: 0,
      session_count: 4,
      cost: null,
      unpriced: true,
      daily_average_tokens: 100,
      daily_average_cost: null,
    };
    expect(weeklyWindowTable([window]).rows[0]).toEqual([
      "Codex",
      "2026-08-11T00:00:00Z",
      "2026-08-18T00:00:00Z",
      700,
      4,
      "",
      100,
      "",
    ]);
  });
});

const analytics: ApplicationAnalyticsDto = {
  summary: {
    total_tokens: 30,
    session_count: 2,
    cache_hit_rate: 0.25,
    average_session_tokens: 15,
    reasoning_share: 0.1,
  },
  by_application: [
    {
      source: "claude",
      application: "Claude Code",
      metrics: {
        total_tokens: 20,
        session_count: 1,
        cache_hit_rate: 0.5,
        average_session_tokens: 20,
        reasoning_share: 0.2,
      },
    },
    {
      source: "codex",
      application: "Codex",
      metrics: {
        total_tokens: 10,
        session_count: 1,
        cache_hit_rate: null,
        average_session_tokens: 10,
        reasoning_share: null,
      },
    },
  ],
  trend: [],
  projects: [
    { project: "/Users/dev/app", total_tokens: 30, values: { claude: 20, codex: 10 } },
  ],
};

describe("application tables", () => {
  it("exports efficiency rows with blank null ratios", () => {
    const table = applicationEfficiencyTable(analytics);
    expect(table.rows[1]).toEqual(["Codex", 10, 1, 10, "", ""]);
  });

  it("exports project matrix with application columns", () => {
    const table = applicationProjectMatrixTable(analytics);
    expect(table.headers).toEqual(["项目", "Claude Code", "Codex", "总计"]);
    expect(table.rows[0][0]).toBe("app");
    expect(table.rows[0].slice(1)).toEqual([20, 10, 30]);
  });
});

describe("cursor export tables", () => {
  it("exports code volume summary", () => {
    const data: CodeVolumeSummary = {
      commit_count: 3,
      lines_added: 100,
      composer_lines_added: 40,
      human_lines_added: 60,
      ai_percentage: 40,
      total_cost: 8,
      cost_unpriced: false,
      cost_per_thousand_ai_lines: 200,
    };
    expect(codeVolumeTable(data).rows).toEqual([
      ["提交数", 3, ""],
      ["新增行", 100, ""],
      ["AI 生成行", 40, ""],
      ["人工编写行", 60, ""],
      ["AI 占比", 40, ""],
      ["全部来源累计费用", 8, ""],
      ["每千行 AI 代码成本", 200, ""],
    ]);
  });

  it("marks code volume cost cells as unpriced and empty when cost is unknown", () => {
    const data: CodeVolumeSummary = {
      commit_count: 0,
      lines_added: 0,
      composer_lines_added: 0,
      human_lines_added: 0,
      ai_percentage: null,
      total_cost: null,
      cost_unpriced: true,
      cost_per_thousand_ai_lines: null,
    };
    const table = codeVolumeTable(data);
    expect(table.headers).toEqual(["指标", "数值", "未定价"]);
    expect(table.rows.slice(-2)).toEqual([
      ["全部来源累计费用", "", "是"],
      ["每千行 AI 代码成本", "", "是"],
    ]);
  });

  it("exports account daily and model tables", () => {
    const data: CursorAccountUsageDto = {
      as_of: "2026-08-18T00:00:00Z",
      event_count: 2,
      input_tokens: 8,
      output_tokens: 2,
      cache_read_tokens: 0,
      cache_creation_tokens: 0,
      total_tokens: 10,
      daily: [
        {
          bucket: "2026-08-17",
          total_tokens: 10,
          input_tokens: 8,
          output_tokens: 2,
          cost: null,
        },
      ],
      by_model: [{ name: "gpt-5", total_tokens: 10, share: 1, cost: null, unpriced: false }],
      headless_tokens: 4,
      interactive_tokens: 6,
      headless_share: 0.4,
    };
    expect(cursorAccountDailyTable(data).rows[0]).toEqual(["2026-08-17", 10, 8, 2]);
    expect(cursorAccountModelTable(data).rows[0]).toEqual(["gpt-5", 10, 1]);
  });

  it("exports session project and tool tables", () => {
    const data: CursorSessionSummaryDto = {
      as_of: null,
      session_count: 2,
      turn_count: 5,
      error_rate: 0,
      active_project_count: 1,
      by_project: [
        {
          name: "/tmp/demo",
          session_count: 2,
          turn_count: 5,
          error_count: 0,
          files_touched: 0,
          last_seen_at: null,
        },
      ],
      by_model: [],
      top_tools: [{ name: "read", call_count: 9 }],
      daily: [],
    };
    expect(cursorSessionProjectTable(data).rows[0]).toEqual(["demo", 2, 5]);
    expect(cursorSessionToolTable(data).rows[0]).toEqual(["read", 9]);
  });
});
