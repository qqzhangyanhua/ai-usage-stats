use crate::domain::{
    GlobalInstructionFile, GlobalInstructionSourceRow, InstructionCheckupFinding,
    InstructionCheckupKind, InstructionCheckupSeverity, InstructionEntryKind,
    InstructionLoadStatus,
};

/// 已查证的用户级合计上限。查不到的 Source 不报截断，避免假阳性。
/// Codex：官方 32 KiB。Dsh：官方默认 65,536 字节渲染预算。
const CODEX_LIMIT: u64 = 32 * 1024;
const DSH_LIMIT: u64 = 65_536;
const NEAR_LIMIT_NUMERATOR: u64 = 4;
const NEAR_LIMIT_DENOMINATOR: u64 = 5;

pub fn collect(sources: &[GlobalInstructionSourceRow]) -> Vec<InstructionCheckupFinding> {
    let mut findings = Vec::new();
    for row in sources {
        findings.extend(empty_files(row));
        findings.extend(override_shields(row));
        findings.extend(present_unloaded(row));
        findings.extend(size_limit(row));
    }
    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| kind_rank(a.kind).cmp(&kind_rank(b.kind)))
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.display_path.cmp(&b.display_path))
    });
    findings
}

fn empty_files(row: &GlobalInstructionSourceRow) -> Vec<InstructionCheckupFinding> {
    row.files
        .iter()
        .filter(|file| {
            file.kind == InstructionEntryKind::File
                && file.load_status == InstructionLoadStatus::Loaded
                && !file.abs_path.is_empty()
                && file.byte_size == 0
        })
        .map(|file| {
            finding(
                InstructionCheckupKind::Empty,
                InstructionCheckupSeverity::High,
                row,
                &file.display_path,
                format!("{} 存在但是 0 字节，等于没有写过内容。", file.display_path),
                format!(
                    "{} 会把它当全局指令加载，但每次会话都读到空文本，偏好不会生效。",
                    row.application
                ),
            )
        })
        .collect()
}

fn override_shields(row: &GlobalInstructionSourceRow) -> Vec<InstructionCheckupFinding> {
    if !has_loaded_override(row) {
        return Vec::new();
    }
    row.files
        .iter()
        .filter(|file| is_base_agents(file) && file.load_status == InstructionLoadStatus::PresentUnloaded)
        .map(|file| {
            finding(
                InstructionCheckupKind::OverrideShields,
                InstructionCheckupSeverity::Medium,
                row,
                &file.display_path,
                format!(
                    "override 文件存在，因此 {} 被屏蔽。",
                    file.display_path
                ),
                format!(
                    "{} 只读 override。基础文件里的偏好当前不会生效；忘了关掉临时覆盖就会一直吃覆盖内容。",
                    row.application
                ),
            )
        })
        .collect()
}

fn present_unloaded(row: &GlobalInstructionSourceRow) -> Vec<InstructionCheckupFinding> {
    let shielded = has_loaded_override(row);
    row.files
        .iter()
        .filter(|file| {
            file.kind == InstructionEntryKind::File
                && file.load_status == InstructionLoadStatus::PresentUnloaded
                && !(shielded && is_base_agents(file))
        })
        .map(|file| {
            finding(
                InstructionCheckupKind::PresentUnloaded,
                InstructionCheckupSeverity::High,
                row,
                &file.display_path,
                format!(
                    "{} 在磁盘上，但原生 {} 不会加载。",
                    file.display_path, row.application
                ),
                format!(
                    "改这份文件不会改变 {} 的行为，容易误以为指令已经生效。",
                    row.application
                ),
            )
        })
        .collect()
}

fn size_limit(row: &GlobalInstructionSourceRow) -> Vec<InstructionCheckupFinding> {
    let Some(limit) = byte_limit(&row.source) else {
        return Vec::new();
    };
    let loaded: u64 = row
        .files
        .iter()
        .filter(|file| {
            file.kind == InstructionEntryKind::File
                && file.load_status == InstructionLoadStatus::Loaded
        })
        .map(|file| file.byte_size)
        .sum();
    if loaded > limit {
        return vec![finding(
            InstructionCheckupKind::OverLimit,
            InstructionCheckupSeverity::Critical,
            row,
            "",
            format!(
                "{} 已加载的全局指令合计 {loaded} 字节，已超过 {limit} 字节上限。",
                row.application
            ),
            "超出部分会被静默截断，排在后面的指令等于没写。".into(),
        )];
    }
    if loaded * NEAR_LIMIT_DENOMINATOR >= limit * NEAR_LIMIT_NUMERATOR {
        return vec![finding(
            InstructionCheckupKind::NearLimit,
            InstructionCheckupSeverity::Low,
            row,
            "",
            format!(
                "{} 已加载的全局指令合计 {loaded} 字节，已接近 {limit} 字节上限。",
                row.application
            ),
            "再往里加内容就可能被静默截断。".into(),
        )];
    }
    Vec::new()
}

fn kind_rank(kind: InstructionCheckupKind) -> u8 {
    match kind {
        InstructionCheckupKind::OverLimit => 0,
        InstructionCheckupKind::Empty => 1,
        InstructionCheckupKind::PresentUnloaded => 2,
        InstructionCheckupKind::OverrideShields => 3,
        InstructionCheckupKind::NearLimit => 4,
    }
}

fn byte_limit(source: &str) -> Option<u64> {
    match source {
        "codex" => Some(CODEX_LIMIT),
        "dsh" => Some(DSH_LIMIT),
        _ => None,
    }
}

fn has_loaded_override(row: &GlobalInstructionSourceRow) -> bool {
    row.files.iter().any(|file| {
        file.load_status == InstructionLoadStatus::Loaded
            && file.display_path.ends_with("AGENTS.override.md")
    })
}

fn is_base_agents(file: &GlobalInstructionFile) -> bool {
    file.display_path.ends_with("AGENTS.md") && !file.display_path.contains("override")
}

fn finding(
    kind: InstructionCheckupKind,
    severity: InstructionCheckupSeverity,
    row: &GlobalInstructionSourceRow,
    display_path: &str,
    problem: String,
    consequence: String,
) -> InstructionCheckupFinding {
    InstructionCheckupFinding {
        kind,
        severity,
        source: row.source.clone(),
        application: row.application.clone(),
        display_path: display_path.to_string(),
        problem,
        consequence,
    }
}
