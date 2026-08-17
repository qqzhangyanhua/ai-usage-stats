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

export type BillingWindowsDto = {
  now: string;
  window_hours: number;
  current: BillingWindowDto[];
  recent: BillingWindowDto[];
};

export type Grain = "day" | "week" | "month";

export type View =
  | "overview"
  | "trend"
  | "application"
  | "model"
  | "provider"
  | "project"
  | "sessions"
  | "cursor"
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

export type SessionSortKey = "tokens" | "session" | "application" | "project" | "time";
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
  cost_note: string | null;
};

export type PriceEntry = {
  model: string;
  provider: string | null;
  input: number;
  output: number;
  cache_read: number;
  cache_creation: number;
};

export type PriceTable = {
  prices: PriceEntry[];
};

export type CodeVolumeSummary = {
  commit_count: number;
  lines_added: number;
  composer_lines_added: number;
  human_lines_added: number;
  ai_percentage: number | null;
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
};

export type IngestReport = {
  files_seen: number;
  files_parsed: number;
  files_skipped: number;
  files_failed: number;
  records_written: number;
  records_removed: number;
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
};
