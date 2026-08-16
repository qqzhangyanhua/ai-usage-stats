export type Filter = {
  from: string | null;
  to: string | null;
  sources: string[];
  models: string[];
  projects: string[];
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

export type SeriesPoint = {
  bucket: string;
  total_tokens: number;
  cost: number | null;
};

export type NamedAmount = {
  name: string;
  total_tokens: number;
  share: number;
  cost: number | null;
  unpriced: boolean;
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
};

export type IngestReport = {
  files_seen: number;
  files_parsed: number;
  files_skipped: number;
  records_written: number;
};
