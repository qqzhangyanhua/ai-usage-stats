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
    Copilot,
}

impl Source {
    pub const ALL: [Source; 12] = [
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
        Source::Copilot,
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
            Source::Copilot => "copilot",
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
            Source::Copilot => "GitHub Copilot CLI",
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
            "copilot" => Some(Source::Copilot),
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

/// 用户配置的月度预算（美元），持久化在独立文件里，与单价表分开管理。
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub monthly_usd: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficialQuotaProvider {
    Claude,
    Codex,
    Cursor,
}

impl OfficialQuotaProvider {
    pub const ALL: [Self; 3] = [Self::Claude, Self::Codex, Self::Cursor];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Cursor => "Cursor",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficialQuotaFreshness {
    Official,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfficialQuotaWindow {
    pub kind: String,
    pub label: String,
    pub used_percent: Option<f64>,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfficialQuotaRow {
    pub provider: String,
    pub application: String,
    pub windows: Vec<OfficialQuotaWindow>,
    pub freshness: OfficialQuotaFreshness,
    pub captured_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfficialQuotaConfig {
    #[serde(default = "default_alerts_enabled")]
    pub alerts_enabled: bool,
}

fn default_alerts_enabled() -> bool {
    true
}

impl Default for OfficialQuotaConfig {
    fn default() -> Self {
        Self {
            alerts_enabled: true,
        }
    }
}

/// 账号级官方额度快照：不进消耗记录，不与本机 5 小时/7 天估计窗混条。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfficialQuotaDto {
    pub rows: Vec<OfficialQuotaRow>,
    pub alerts_enabled: bool,
    pub stale_after_minutes: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfficialQuotaHookDto {
    pub settings_path: String,
    pub command: String,
    pub snippet: String,
    pub already_configured: bool,
    pub conflict: bool,
    pub conflict_command: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionLoadStatus {
    Loaded,
    PresentUnloaded,
    LocallyInvisible,
    NotCreated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionEvidence {
    Verified,
    Inferred,
    NoMechanism,
}

/// 各 Source 的消耗摘要，扫描接缝预留给用量交叉洞察，本维度不写入 sqlite。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InstructionUsageSummary {
    pub sources: Vec<InstructionSourceUsage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstructionSourceUsage {
    pub source: String,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalInstructionFile {
    pub display_path: String,
    pub abs_path: String,
    pub byte_size: u64,
    pub modified_at: Option<String>,
    pub load_status: InstructionLoadStatus,
    pub evidence: InstructionEvidence,
    pub content: String,
    pub error: Option<String>,
    pub note: Option<String>,
    pub action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalInstructionSourceRow {
    pub source: String,
    pub application: String,
    pub files: Vec<GlobalInstructionFile>,
}

/// 全局指令快照：不进消耗记录，不进 Token KPI，不写 sqlite。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalInstructionDto {
    pub sources: Vec<GlobalInstructionSourceRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteUserFileRequest {
    pub abs_path: String,
    pub content: String,
    pub expected_mtime: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteUserFileResult {
    pub modified_at: Option<String>,
    pub byte_size: u64,
}

/// 当前自然月的预算执行情况：仅本地估算，非官方账单，用于阈值提醒与设置页展示。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetStatusDto {
    pub monthly_budget: Option<f64>,
    pub month: String,
    pub days_elapsed: i64,
    pub days_in_month: i64,
    pub month_to_date_cost: f64,
    pub unpriced: bool,
    pub projected_month_cost: Option<f64>,
    pub percent_used: Option<f64>,
    pub percent_projected: Option<f64>,
    pub thresholds: Vec<u32>,
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

/// 按来源统计的 7 天滚动窗口：不像 5 小时窗那样按活动间隔切块，而是持续滚动的
/// "最近 N 天用了多少"，贴近 Claude 等工具的周度限额心智模型（仅本地估计，非官方配额）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeeklyWindowDto {
    pub source: String,
    pub application: String,
    pub window_days: i64,
    pub start: String,
    pub end: String,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub reasoning_tokens: i64,
    pub session_count: i64,
    pub cost: Option<f64>,
    pub unpriced: bool,
    pub daily_average_tokens: f64,
    pub daily_average_cost: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BillingWindowsDto {
    pub now: String,
    pub window_hours: i64,
    pub current: Vec<BillingWindowDto>,
    pub recent: Vec<BillingWindowDto>,
    pub weekly_window_days: i64,
    pub weekly: Vec<WeeklyWindowDto>,
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
    /// 列表与导出都可打开；打开后对聚合结果做价目 JOIN。
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
    /// 本轮费用来自哪一层：来源自带 / 用户单价 / LiteLLM 快照 / 未配置。
    pub cost_source: CostSource,
    pub cost_note: Option<String>,
}

/// 价目条目来源。缺省为用户配置，兼容旧 `prices.json`。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PriceOrigin {
    #[default]
    User,
    Snapshot,
}

impl PriceOrigin {
    pub fn is_user(&self) -> bool {
        matches!(self, PriceOrigin::User)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PriceOrigin::User => "user",
            PriceOrigin::Snapshot => "snapshot",
        }
    }
}

/// 单条消耗记录的费用来源，给界面展示用。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CostSource {
    Native,
    User,
    Snapshot,
    #[default]
    None,
}

impl CostSource {
    pub fn as_str(self) -> &'static str {
        match self {
            CostSource::Native => "native",
            CostSource::User => "user",
            CostSource::Snapshot => "snapshot",
            CostSource::None => "none",
        }
    }

    pub fn from_sql(value: &str) -> Self {
        match value {
            "native" => CostSource::Native,
            "user" => CostSource::User,
            "snapshot" => CostSource::Snapshot,
            _ => CostSource::None,
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            CostSource::Native => "来源自带",
            CostSource::User => "用户单价",
            CostSource::Snapshot => "LiteLLM 快照",
            CostSource::None => "单价未配置",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceEntry {
    pub model: String,
    pub provider: Option<String>,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_creation: f64,
    /// 旧文件没有该字段时视为用户单价。
    #[serde(default, skip_serializing_if = "PriceOrigin::is_user")]
    pub origin: PriceOrigin,
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
    pub cost_source: CostSource,
}

impl DerivedCost {
    pub fn cost_note(&self) -> String {
        self.cost_source.note().to_string()
    }
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
    pub commit_message: String,
    pub lines_added: i64,
    pub lines_deleted: i64,
    pub composer_lines_added: i64,
    pub composer_lines_deleted: i64,
    pub human_lines_added: i64,
    pub human_lines_deleted: i64,
    pub tab_lines_added: i64,
    pub tab_lines_deleted: i64,
    pub ai_percentage: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeVolumeDailyPoint {
    pub bucket: String,
    pub lines_added: i64,
    pub lines_deleted: i64,
    pub composer_lines_added: i64,
    pub tab_lines_added: i64,
    pub human_lines_added: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeVolumeBranchRow {
    pub name: String,
    pub commit_count: i64,
    pub lines_added: i64,
    pub composer_lines_added: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeVolumeSummary {
    pub commit_count: i64,
    pub lines_added: i64,
    pub lines_deleted: i64,
    pub net_lines: i64,
    pub composer_lines_added: i64,
    pub composer_lines_deleted: i64,
    pub human_lines_added: i64,
    pub human_lines_deleted: i64,
    pub tab_lines_added: i64,
    pub tab_lines_deleted: i64,
    pub ai_percentage: Option<f64>,
    /// 全部时间、全部来源的消耗记录费用估算；与代码量一样按「至今累计」口径，不受总览筛选影响。
    /// 只用于下面的粗略 ROI 交叉指标，不代表 Cursor 单一来源的花费。
    pub total_cost: Option<f64>,
    pub cost_unpriced: bool,
    /// = total_cost ÷ (composer_lines_added / 1000)。跨来源粗略口径：分子是全部 AI CLI 的费用，
    /// 分母只是 Cursor 记录到的 AI 生成行数，两者不是同一统计边界，仅供参考，不做精确归因。
    pub cost_per_thousand_ai_lines: Option<f64>,
    pub daily: Vec<CodeVolumeDailyPoint>,
    pub by_branch: Vec<CodeVolumeBranchRow>,
    pub commits: Vec<CodeVolumeCommit>,
}

impl CodeVolumeSummary {
    pub fn empty() -> Self {
        Self {
            commit_count: 0,
            lines_added: 0,
            lines_deleted: 0,
            net_lines: 0,
            composer_lines_added: 0,
            composer_lines_deleted: 0,
            human_lines_added: 0,
            human_lines_deleted: 0,
            tab_lines_added: 0,
            tab_lines_deleted: 0,
            ai_percentage: None,
            total_cost: None,
            cost_unpriced: false,
            cost_per_thousand_ai_lines: None,
            daily: Vec::new(),
            by_branch: Vec::new(),
            commits: Vec::new(),
        }
    }
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
    pub user_prompt_count: i64,
    pub subagent_count: i64,
    pub tool_calls_json: String,
    pub models_json: String,
    pub sources_json: String,
    pub extensions_json: String,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub files_touched: i64,
    pub source_file: String,
}

/// 单条 Cursor 会话的界面明细（已解析 models / 工具次数，不含正文）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionListRow {
    pub session_id: String,
    pub project: String,
    pub turn_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub aborted_count: i64,
    pub user_prompt_count: i64,
    pub subagent_count: i64,
    pub models: Vec<String>,
    pub sources: Vec<String>,
    pub tool_call_count: i64,
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
    pub error_count: i64,
    pub files_touched: i64,
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionDailyPoint {
    pub bucket: String,
    pub session_count: i64,
    pub turn_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionModelRow {
    pub name: String,
    pub session_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionToolRow {
    pub name: String,
    pub call_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionSourceRow {
    pub name: String,
    pub session_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionExtensionRow {
    pub name: String,
    pub file_count: i64,
}

/// Cursor 会话汇总：独立维度，不并入本机 token 总量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionSummaryDto {
    pub as_of: Option<String>,
    pub session_count: i64,
    pub turn_count: i64,
    pub aborted_count: i64,
    pub user_prompt_count: i64,
    pub subagent_count: i64,
    pub error_rate: Option<f64>,
    pub average_turns: Option<f64>,
    pub average_tools_per_turn: Option<f64>,
    pub write_read_ratio: Option<f64>,
    pub active_project_count: i64,
    pub by_project: Vec<CursorSessionProjectRow>,
    pub by_model: Vec<CursorSessionModelRow>,
    pub by_source: Vec<CursorSessionSourceRow>,
    pub by_extension: Vec<CursorSessionExtensionRow>,
    pub top_tools: Vec<CursorSessionToolRow>,
    pub tool_groups: Vec<CursorSessionToolRow>,
    pub daily: Vec<CursorSessionDailyPoint>,
}

impl CursorSessionSummaryDto {
    pub fn empty() -> Self {
        Self {
            as_of: None,
            session_count: 0,
            turn_count: 0,
            aborted_count: 0,
            user_prompt_count: 0,
            subagent_count: 0,
            error_rate: None,
            average_turns: None,
            average_tools_per_turn: None,
            write_read_ratio: None,
            active_project_count: 0,
            by_project: Vec::new(),
            by_model: Vec::new(),
            by_source: Vec::new(),
            by_extension: Vec::new(),
            top_tools: Vec::new(),
            tool_groups: Vec::new(),
            daily: Vec::new(),
        }
    }
}

/// Cursor 会话列表明细：搜索/项目/排序/分页均下沉到 SQL。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorSessionQuery {
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub sort_by: Option<String>,
    #[serde(default)]
    pub sort_dir: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorSessionPage {
    pub rows: Vec<CursorSessionListRow>,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionHashFile {
    pub path: String,
    pub extension: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorSessionDetailDto {
    pub session: CursorSessionListRow,
    pub tools: Vec<CursorSessionToolRow>,
    pub hash_files: Vec<CursorSessionHashFile>,
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub transcript_missing: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorAccountEventQuery {
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub page_size: Option<u32>,
    #[serde(default)]
    pub sort_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorAccountEventRow {
    pub occurred_at: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
    pub is_headless: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorAccountEventPage {
    pub rows: Vec<CursorAccountEventRow>,
    pub total: u32,
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
    /// 源文件已被工具自身清理，但仍计入统计的历史记录数（见 ADR 0004）。
    pub archived_record_count: u64,
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
    /// 本轮因源文件消失而归档（非物理删除，见 ADR 0004）的记录数。
    pub records_archived: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestReport {
    pub files_seen: u64,
    pub files_parsed: u64,
    pub files_skipped: u64,
    pub files_failed: u64,
    pub records_written: u64,
    pub records_removed: u64,
    pub records_archived: u64,
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
            records_archived: 0,
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
                    records_archived: 0,
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
