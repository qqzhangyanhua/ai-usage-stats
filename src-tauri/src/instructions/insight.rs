use crate::domain::{
    GlobalInstructionFile, GlobalInstructionSourceRow, InstructionEntryKind, InstructionEvidence,
    InstructionImbalance, InstructionInvestment, InstructionLoadStatus, InstructionUsageSummary,
};

const MIN_TOTAL_TOKENS: i64 = 1_000;
const HIGH_SHARE_NUM: i64 = 2;
const HIGH_SHARE_DEN: i64 = 5;
const LOW_BYTES: u64 = 1_024;

pub fn collect(
    sources: &[GlobalInstructionSourceRow],
    usage: &InstructionUsageSummary,
) -> (Vec<InstructionInvestment>, Vec<InstructionImbalance>) {
    let tokens_by_source: std::collections::BTreeMap<&str, i64> = usage
        .sources
        .iter()
        .map(|row| (row.source.as_str(), row.total_tokens))
        .collect();
    let mut investments: Vec<InstructionInvestment> = sources
        .iter()
        .map(|row| {
            let (loaded_bytes, modified_at) = loaded_investment(row);
            InstructionInvestment {
                source: row.source.clone(),
                application: row.application.clone(),
                loaded_bytes,
                modified_at,
                total_tokens: tokens_by_source
                    .get(row.source.as_str())
                    .copied()
                    .unwrap_or(0),
            }
        })
        .collect();
    investments.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| a.application.cmp(&b.application))
    });

    let total_tokens: i64 = usage.sources.iter().map(|row| row.total_tokens).sum();
    if total_tokens < MIN_TOTAL_TOKENS {
        return (investments, Vec::new());
    }

    let mut imbalances = Vec::new();
    for row in &investments {
        let Some(source) = sources.iter().find(|item| item.source == row.source) else {
            continue;
        };
        if !can_judge_investment(source) {
            continue;
        }
        if !share_at_least(
            row.total_tokens,
            total_tokens,
            HIGH_SHARE_NUM,
            HIGH_SHARE_DEN,
        ) {
            continue;
        }
        if row.loaded_bytes >= LOW_BYTES {
            continue;
        }
        let percent = row.total_tokens.saturating_mul(100) / total_tokens;
        imbalances.push(InstructionImbalance {
            source: row.source.clone(),
            application: row.application.clone(),
            note: format!(
                "{} 占本机用量的 {percent}%，已加载的全局指令只有 {} 字节。用量高的来源，指令投入明显偏低。",
                row.application, row.loaded_bytes
            ),
        });
    }
    (investments, imbalances)
}

fn loaded_investment(row: &GlobalInstructionSourceRow) -> (u64, Option<String>) {
    let loaded: Vec<&GlobalInstructionFile> = row
        .files
        .iter()
        .filter(|file| {
            file.kind == InstructionEntryKind::File
                && file.load_status == InstructionLoadStatus::Loaded
        })
        .collect();
    let loaded_bytes = loaded.iter().map(|file| file.byte_size).sum();
    let modified_at = loaded
        .iter()
        .filter_map(|file| file.modified_at.clone())
        .max();
    (loaded_bytes, modified_at)
}

fn can_judge_investment(row: &GlobalInstructionSourceRow) -> bool {
    row.files.iter().any(|file| {
        file.evidence != InstructionEvidence::NoMechanism
            && file.load_status != InstructionLoadStatus::LocallyInvisible
    })
}

fn share_at_least(tokens: i64, total: i64, num: i64, den: i64) -> bool {
    total > 0 && tokens.saturating_mul(den) >= total.saturating_mul(num)
}
