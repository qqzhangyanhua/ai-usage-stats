use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Codex,
    Claude,
    Pi,
    Opencode,
    Kimi,
    Dsh,
    Gemini,
    Grok,
    Qwen,
    Factory,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Codex => "codex",
            Source::Claude => "claude",
            Source::Pi => "pi",
            Source::Opencode => "opencode",
            Source::Kimi => "kimi",
            Source::Dsh => "dsh",
            Source::Gemini => "gemini",
            Source::Grok => "grok",
            Source::Qwen => "qwen",
            Source::Factory => "factory",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "codex" => Some(Source::Codex),
            "claude" => Some(Source::Claude),
            "pi" => Some(Source::Pi),
            "opencode" => Some(Source::Opencode),
            "kimi" => Some(Source::Kimi),
            "dsh" => Some(Source::Dsh),
            "gemini" => Some(Source::Gemini),
            "grok" => Some(Source::Grok),
            "qwen" => Some(Source::Qwen),
            "factory" => Some(Source::Factory),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub occurred_at: String,
    pub source: Source,
    pub model: String,
    pub provider: String,
    pub project: String,
    pub session_id: String,
    pub source_file: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub reasoning_tokens: i64,
    pub total_tokens: i64,
    pub native_cost: Option<f64>,
}

impl UsageRecord {
    pub fn with_total(mut self) -> Self {
        if self.total_tokens <= 0 {
            self.total_tokens = self.input_tokens
                + self.output_tokens
                + self.cache_read_tokens
                + self.cache_creation_tokens
                + self.reasoning_tokens;
        }
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    pub from: Option<String>,
    pub to: Option<String>,
    pub sources: Vec<String>,
    pub models: Vec<String>,
    pub projects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverviewDto {
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub reasoning_tokens: i64,
    pub session_count: i64,
    pub cost: Option<f64>,
    pub unpriced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesPoint {
    pub bucket: String,
    pub total_tokens: i64,
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedAmount {
    pub name: String,
    pub total_tokens: i64,
    pub share: f64,
    pub cost: Option<f64>,
    pub unpriced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRow {
    pub session_id: String,
    pub source: String,
    pub project: String,
    pub model: String,
    pub total_tokens: i64,
    pub started_at: String,
    pub ended_at: String,
    pub source_file: String,
    pub cost: Option<f64>,
    pub unpriced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnRow {
    pub occurred_at: String,
    pub model: String,
    pub provider: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub reasoning_tokens: i64,
    pub total_tokens: i64,
    pub source_file: String,
    pub cost: Option<f64>,
    pub unpriced: bool,
    pub cost_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceEntry {
    pub model: String,
    pub provider: Option<String>,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_creation: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceTable {
    pub prices: Vec<PriceEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedCost {
    pub amount: Option<f64>,
    pub unpriced: bool,
    pub source_native: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeVolumeCommit {
    pub commit_hash: String,
    pub branch: String,
    pub scored_at: String,
    pub lines_added: i64,
    pub composer_lines_added: i64,
    pub human_lines_added: i64,
    pub ai_percentage: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeVolumeSummary {
    pub commit_count: i64,
    pub lines_added: i64,
    pub composer_lines_added: i64,
    pub human_lines_added: i64,
    pub ai_percentage: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestReport {
    pub files_seen: u64,
    pub files_parsed: u64,
    pub files_skipped: u64,
    pub records_written: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterOptions {
    pub sources: Vec<String>,
    pub models: Vec<String>,
    pub projects: Vec<String>,
}
