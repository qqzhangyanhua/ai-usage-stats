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
    CursorAgent,
}

impl Source {
    pub const ALL: [Source; 11] = [
        Source::Codex,
        Source::Claude,
        Source::Pi,
        Source::Opencode,
        Source::Kimi,
        Source::Dsh,
        Source::Gemini,
        Source::Grok,
        Source::Qwen,
        Source::Factory,
        Source::CursorAgent,
    ];

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
            Source::CursorAgent => "cursor_agent",
        }
    }

    pub fn application_name(self) -> &'static str {
        match self {
            Source::Codex => "Codex",
            Source::Claude => "Claude Code",
            Source::Pi => "Pi",
            Source::Opencode => "OpenCode",
            Source::Kimi => "Kimi CLI",
            Source::Dsh => "DeepSeek Harness",
            Source::Gemini => "Gemini CLI",
            Source::Grok => "Grok CLI",
            Source::Qwen => "Qwen Code",
            Source::Factory => "Droid",
            Source::CursorAgent => "Cursor Agent",
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
            "cursor_agent" => Some(Source::CursorAgent),
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
    #[serde(default)]
    pub providers: Vec<String>,
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
pub struct BurnRateDto {
    pub tokens_per_minute: f64,
    pub cost_per_hour: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionDto {
    pub total_tokens: i64,
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BillingWindowDto {
    pub source: String,
    pub application: String,
    pub start: String,
    pub end: String,
    pub last_activity: String,
    pub is_active: bool,
    pub elapsed_minutes: i64,
    pub remaining_minutes: Option<i64>,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub reasoning_tokens: i64,
    pub session_count: i64,
    pub cost: Option<f64>,
    pub unpriced: bool,
    pub burn: Option<BurnRateDto>,
    pub projection: Option<ProjectionDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BillingWindowsDto {
    pub now: String,
    pub window_hours: i64,
    pub current: Vec<BillingWindowDto>,
    pub recent: Vec<BillingWindowDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesPoint {
    pub bucket: String,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
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
pub struct EfficiencyMetrics {
    pub total_tokens: i64,
    pub session_count: i64,
    pub cache_hit_rate: Option<f64>,
    pub average_session_tokens: Option<f64>,
    pub reasoning_share: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplicationEfficiency {
    pub source: String,
    pub application: String,
    pub metrics: EfficiencyMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplicationTrendPoint {
    pub bucket: String,
    pub total_tokens: i64,
    pub values: std::collections::BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectApplicationRow {
    pub project: String,
    pub total_tokens: i64,
    pub values: std::collections::BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplicationAnalyticsDto {
    pub summary: EfficiencyMetrics,
    pub by_application: Vec<ApplicationEfficiency>,
    pub trend: Vec<ApplicationTrendPoint>,
    pub projects: Vec<ProjectApplicationRow>,
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

/// 会话列表分页查询参数：搜索/排序/分页均下沉到 SQL 层执行。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionQuery {
    pub filter: Filter,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub sort_by: Option<String>,
    #[serde(default)]
    pub sort_dir: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub page_size: Option<u32>,
    /// 列表 UI 不展示费用；仅导出时打开，避免对全表做价目 JOIN。
    #[serde(default)]
    pub include_cost: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPage {
    pub rows: Vec<SessionRow>,
    pub total: u32,
    pub total_tokens: i64,
    pub last_ended: Option<String>,
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

/// 内置/可刷新的价目快照（当前来自 LiteLLM 社区维护的 `model_prices_and_context_window.json`）。
/// 作为「用户单价 + 来源自带费用」之外的兜底：只在某模型既无 native_cost、用户也未配置单价时启用，
/// 让费用从「用户手填才能算」变成「开箱大体准」。快照里的 `provider` 一律为空，充当按模型的兜底单价。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceSnapshot {
    pub as_of: String,
    pub source: String,
    pub entries: Vec<PriceEntry>,
}

/// 给界面展示的快照元信息（不含逐条单价）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceSnapshotMeta {
    pub as_of: String,
    pub source: String,
    pub count: usize,
    /// 是否为内置默认快照（`true`）还是用户联网刷新后的本地缓存（`false`）。
    pub bundled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedCost {
    pub amount: Option<f64>,
    pub unpriced: bool,
    pub source_native: bool,
}

/// Cursor 账号级用量事件：来自云端仪表盘，不是本机会话文件。
/// 独立于 `UsageRecord`，不含 session_id / source_file。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorUsageEvent {
    pub occurred_at: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub is_headless: bool,
}

impl CursorUsageEvent {
    pub fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.cache_read_tokens + self.cache_creation_tokens
    }

    pub fn fingerprint(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.occurred_at,
            self.model,
            self.input_tokens,
            self.output_tokens,
            self.cache_read_tokens,
            self.cache_creation_tokens,
            self.is_headless
        )
    }
}

/// Cursor 账号用量聚合：独立维度，不并入本机 token 总量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorAccountUsageDto {
    pub as_of: Option<String>,
    pub event_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
    pub daily: Vec<SeriesPoint>,
    pub by_model: Vec<NamedAmount>,
    pub headless_tokens: i64,
    pub interactive_tokens: i64,
    pub headless_share: Option<f64>,
}

impl CursorAccountUsageDto {
    pub fn empty() -> Self {
        Self {
            as_of: None,
            event_count: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            total_tokens: 0,
            daily: Vec::new(),
            by_model: Vec::new(),
            headless_tokens: 0,
            interactive_tokens: 0,
            headless_share: None,
        }
    }
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

/// 单条 Cursor 会话聚合（本机 agent-transcripts，不含正文）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionRecord {
    pub session_id: String,
    pub project: String,
    pub turn_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub aborted_count: i64,
    pub tool_calls_json: String,
    pub models_json: String,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub files_touched: i64,
    pub source_file: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionProjectRow {
    pub name: String,
    pub session_count: i64,
    pub turn_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionDailyPoint {
    pub bucket: String,
    pub session_count: i64,
    pub turn_count: i64,
}

/// Cursor 会话汇总：独立维度，不并入本机 token 总量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionSummaryDto {
    pub as_of: Option<String>,
    pub session_count: i64,
    pub turn_count: i64,
    pub error_rate: Option<f64>,
    pub active_project_count: i64,
    pub by_project: Vec<CursorSessionProjectRow>,
    pub daily: Vec<CursorSessionDailyPoint>,
}

impl CursorSessionSummaryDto {
    pub fn empty() -> Self {
        Self {
            as_of: None,
            session_count: 0,
            turn_count: 0,
            error_rate: None,
            active_project_count: 0,
            by_project: Vec::new(),
            daily: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceDiagnostic {
    pub source: String,
    pub application: String,
    pub detected: bool,
    pub root_path: String,
    pub cached_files: u64,
    pub record_count: u64,
    pub total_tokens: i64,
    pub coverage: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestIssue {
    pub source: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceIngestReport {
    pub source: String,
    pub detected: bool,
    pub files_seen: u64,
    pub files_parsed: u64,
    pub files_skipped: u64,
    pub files_failed: u64,
    pub records_written: u64,
    pub records_removed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestReport {
    pub files_seen: u64,
    pub files_parsed: u64,
    pub files_skipped: u64,
    pub files_failed: u64,
    pub records_written: u64,
    pub records_removed: u64,
    pub partial_success: bool,
    pub issues: Vec<IngestIssue>,
    pub sources: Vec<SourceIngestReport>,
}

impl Default for IngestReport {
    fn default() -> Self {
        Self {
            files_seen: 0,
            files_parsed: 0,
            files_skipped: 0,
            files_failed: 0,
            records_written: 0,
            records_removed: 0,
            partial_success: false,
            issues: Vec::new(),
            sources: Source::ALL
                .iter()
                .map(|source| SourceIngestReport {
                    source: source.as_str().to_string(),
                    detected: false,
                    files_seen: 0,
                    files_parsed: 0,
                    files_skipped: 0,
                    files_failed: 0,
                    records_written: 0,
                    records_removed: 0,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterOptions {
    pub sources: Vec<String>,
    pub models: Vec<String>,
    pub projects: Vec<String>,
    pub providers: Vec<String>,
}
