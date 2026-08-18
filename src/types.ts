export type Filter = {
  from: string | null;
  to: string | null;
  sources: string[];
  models: string[];
  projects: string[];
  providers: string[];
};

export type OverviewDto = {
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  reasoning_tokens: number;
  session_count: number;
  cost: number | null;
  unpriced: boolean;
};

export type BurnRateDto = {
  tokens_per_minute: number;
  cost_per_hour: number | null;
};

export type ProjectionDto = {
  total_tokens: number;
  cost: number | null;
};

export type BillingWindowDto = {
  source: string;
  application: string;
  start: string;
  end: string;
  last_activity: string;
  is_active: boolean;
  elapsed_minutes: number;
  remaining_minutes: number | null;
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  reasoning_tokens: number;
  session_count: number;
  cost: number | null;
  unpriced: boolean;
  burn: BurnRateDto | null;
  projection: ProjectionDto | null;
};

export type WeeklyWindowDto = {
  source: string;
  application: string;
  window_days: number;
  start: string;
  end: string;
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  reasoning_tokens: number;
  session_count: number;
  cost: number | null;
  unpriced: boolean;
  daily_average_tokens: number;
  daily_average_cost: number | null;
};

export type BillingWindowsDto = {
  now: string;
  window_hours: number;
  current: BillingWindowDto[];
  recent: BillingWindowDto[];
  weekly_window_days: number;
  weekly: WeeklyWindowDto[];
};

export type BudgetConfig = {
  monthly_usd: number | null;
};

export type BudgetStatusDto = {
  monthly_budget: number | null;
  month: string;
  days_elapsed: number;
  days_in_month: number;
  month_to_date_cost: number;
  unpriced: boolean;
  projected_month_cost: number | null;
  percent_used: number | null;
  percent_projected: number | null;
  thresholds: number[];
};

export type Grain = "hour" | "day" | "week" | "month";

export type View =
  | "overview"
  | "trend"
  | "application"
  | "model"
  | "provider"
  | "project"
  | "sessions"
  | "cursor"
  | "cursor-sessions"
  | "settings";

export type SeriesPoint = {
  bucket: string;
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cost: number | null;
};

export type NamedAmount = {
  name: string;
  total_tokens: number;
  share: number;
  cost: number | null;
  unpriced: boolean;
};

export type EfficiencyMetrics = {
  total_tokens: number;
  session_count: number;
  cache_hit_rate: number | null;
  average_session_tokens: number | null;
  reasoning_share: number | null;
};

export type ApplicationEfficiency = {
  source: string;
  application: string;
  metrics: EfficiencyMetrics;
};

export type ApplicationTrendPoint = {
  bucket: string;
  total_tokens: number;
  values: Record<string, number>;
};

export type ProjectApplicationRow = {
  project: string;
  total_tokens: number;
  values: Record<string, number>;
};

export type ApplicationAnalyticsDto = {
  summary: EfficiencyMetrics;
  by_application: ApplicationEfficiency[];
  trend: ApplicationTrendPoint[];
  projects: ProjectApplicationRow[];
};

export type SessionRow = {
  session_id: string;
  source: string;
  project: string;
  model: string;
  total_tokens: number;
  started_at: string;
  ended_at: string;
  source_file: string;
  cost: number | null;
  unpriced: boolean;
};

export type SessionSortKey =
  | "tokens"
  | "session"
  | "application"
  | "project"
  | "model"
  | "cost"
  | "time";
export type SortDir = "asc" | "desc";

export type SessionQuery = {
  filter: Filter;
  search?: string | null;
  sortBy?: SessionSortKey | null;
  sortDir?: SortDir | null;
  page?: number;
  pageSize?: number;
  includeCost?: boolean;
};

export type SessionPage = {
  rows: SessionRow[];
  total: number;
  totalTokens: number;
  lastEnded: string | null;
};

export type CostSource = "native" | "user" | "snapshot" | "none";

export type PriceOrigin = "user" | "snapshot";

export type TurnRow = {
  occurred_at: string;
  model: string;
  provider: string;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  reasoning_tokens: number;
  total_tokens: number;
  source_file: string;
  cost: number | null;
  unpriced: boolean;
  cost_source: CostSource;
  cost_note: string | null;
};

export type PriceEntry = {
  model: string;
  provider: string | null;
  input: number;
  output: number;
  cache_read: number;
  cache_creation: number;
  origin?: PriceOrigin;
};

export type PriceTable = {
  prices: PriceEntry[];
};

export type PriceSnapshotMeta = {
  as_of: string;
  source: string;
  count: number;
  bundled: boolean;
};

export type CodeVolumeCommit = {
  commit_hash: string;
  branch: string;
  scored_at: string;
  commit_message: string;
  lines_added: number;
  lines_deleted: number;
  composer_lines_added: number;
  composer_lines_deleted: number;
  human_lines_added: number;
  human_lines_deleted: number;
  tab_lines_added: number;
  tab_lines_deleted: number;
  ai_percentage: number | null;
};

export type CodeVolumeDailyPoint = {
  bucket: string;
  lines_added: number;
  lines_deleted: number;
  composer_lines_added: number;
  tab_lines_added: number;
  human_lines_added: number;
};

export type CodeVolumeBranchRow = {
  name: string;
  commit_count: number;
  lines_added: number;
  composer_lines_added: number;
};

export type CodeVolumeSummary = {
  commit_count: number;
  lines_added: number;
  lines_deleted: number;
  net_lines: number;
  composer_lines_added: number;
  composer_lines_deleted: number;
  human_lines_added: number;
  human_lines_deleted: number;
  tab_lines_added: number;
  tab_lines_deleted: number;
  ai_percentage: number | null;
  total_cost: number | null;
  cost_unpriced: boolean;
  cost_per_thousand_ai_lines: number | null;
  daily: CodeVolumeDailyPoint[];
  by_branch: CodeVolumeBranchRow[];
  commits: CodeVolumeCommit[];
};

export type CursorAccountUsageDto = {
  as_of: string | null;
  event_count: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  total_tokens: number;
  daily: SeriesPoint[];
  by_model: NamedAmount[];
  headless_tokens: number;
  interactive_tokens: number;
  headless_share: number | null;
};

export type CursorSessionSortKey =
  | "session"
  | "project"
  | "model"
  | "turns"
  | "errors"
  | "tools"
  | "files"
  | "time";

export type CursorSessionListRow = {
  session_id: string;
  project: string;
  turn_count: number;
  success_count: number;
  error_count: number;
  aborted_count: number;
  user_prompt_count: number;
  subagent_count: number;
  models: string[];
  sources: string[];
  tool_call_count: number;
  first_seen_at: string | null;
  last_seen_at: string | null;
  files_touched: number;
  source_file: string;
};

export type CursorSessionProjectRow = {
  name: string;
  session_count: number;
  turn_count: number;
  error_count: number;
  files_touched: number;
  last_seen_at: string | null;
};

export type CursorSessionDailyPoint = {
  bucket: string;
  session_count: number;
  turn_count: number;
};

export type CursorSessionModelRow = {
  name: string;
  session_count: number;
};

export type CursorSessionToolRow = {
  name: string;
  call_count: number;
};

export type CursorSessionSourceRow = {
  name: string;
  session_count: number;
};

export type CursorSessionExtensionRow = {
  name: string;
  file_count: number;
};

export type CursorSessionSummaryDto = {
  as_of: string | null;
  session_count: number;
  turn_count: number;
  aborted_count: number;
  user_prompt_count: number;
  subagent_count: number;
  error_rate: number | null;
  average_turns: number | null;
  average_tools_per_turn: number | null;
  write_read_ratio: number | null;
  active_project_count: number;
  by_project: CursorSessionProjectRow[];
  by_model: CursorSessionModelRow[];
  by_source: CursorSessionSourceRow[];
  by_extension: CursorSessionExtensionRow[];
  top_tools: CursorSessionToolRow[];
  tool_groups: CursorSessionToolRow[];
  daily: CursorSessionDailyPoint[];
};

export type CursorSessionQuery = {
  search?: string | null;
  project?: string | null;
  sortBy?: CursorSessionSortKey | null;
  sortDir?: SortDir | null;
  page?: number;
  pageSize?: number;
};

export type CursorSessionPage = {
  rows: CursorSessionListRow[];
  total: number;
};

export type CursorSessionHashFile = {
  path: string;
  extension: string;
  source: string;
};

export type CursorSessionDetailDto = {
  session: CursorSessionListRow;
  tools: CursorSessionToolRow[];
  hash_files: CursorSessionHashFile[];
  read_paths: string[];
  write_paths: string[];
  transcript_missing: boolean;
};

export type CursorAccountEventQuery = {
  page?: number | null;
  pageSize?: number | null;
  sortDir?: SortDir | null;
};

export type CursorAccountEventRow = {
  occurred_at: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  total_tokens: number;
  is_headless: boolean;
};

export type CursorAccountEventPage = {
  rows: CursorAccountEventRow[];
  total: number;
};

export type FilterOptions = {
  sources: string[];
  models: string[];
  projects: string[];
  providers: string[];
};

export type IngestIssue = {
  source: string;
  path: string;
  message: string;
};

export type SourceIngestReport = {
  source: string;
  detected: boolean;
  files_seen: number;
  files_parsed: number;
  files_skipped: number;
  files_failed: number;
  records_written: number;
  records_removed: number;
  records_archived: number;
};

export type IngestReport = {
  files_seen: number;
  files_parsed: number;
  files_skipped: number;
  files_failed: number;
  records_written: number;
  records_removed: number;
  records_archived: number;
  partial_success: boolean;
  issues: IngestIssue[];
  sources: SourceIngestReport[];
};

export type SourceDiagnostic = {
  source: string;
  application: string;
  detected: boolean;
  root_path: string;
  cached_files: number;
  record_count: number;
  total_tokens: number;
  coverage: string;
  archived_record_count: number;
};
