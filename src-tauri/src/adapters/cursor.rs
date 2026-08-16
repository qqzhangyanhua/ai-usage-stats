use crate::domain::{CodeVolumeCommit, CodeVolumeSummary};

#[derive(Debug, Clone)]
pub struct CursorCommitRow {
    pub commit_hash: String,
    pub branch: String,
    pub scored_at_ms: i64,
    pub lines_added: i64,
    pub composer_lines_added: i64,
    pub human_lines_added: i64,
    pub ai_percentage: Option<f64>,
}

pub fn parse_cursor_commits(rows: &[CursorCommitRow]) -> Vec<CodeVolumeCommit> {
    rows.iter()
        .map(|row| CodeVolumeCommit {
            commit_hash: row.commit_hash.clone(),
            branch: row.branch.clone(),
            scored_at: chrono::DateTime::from_timestamp_millis(row.scored_at_ms)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
            lines_added: row.lines_added,
            composer_lines_added: row.composer_lines_added,
            human_lines_added: row.human_lines_added,
            ai_percentage: row.ai_percentage,
        })
        .collect()
}

pub fn summarize_code_volume(commits: &[CodeVolumeCommit]) -> CodeVolumeSummary {
    let commit_count = commits.len() as i64;
    let lines_added = commits.iter().map(|c| c.lines_added).sum();
    let composer_lines_added = commits.iter().map(|c| c.composer_lines_added).sum();
    let human_lines_added = commits.iter().map(|c| c.human_lines_added).sum();
    let ai_percentage = if lines_added > 0 {
        Some((composer_lines_added as f64 / lines_added as f64) * 100.0)
    } else {
        let scored: Vec<f64> = commits.iter().filter_map(|c| c.ai_percentage).collect();
        if scored.is_empty() {
            None
        } else {
            Some(scored.iter().sum::<f64>() / scored.len() as f64)
        }
    };
    CodeVolumeSummary {
        commit_count,
        lines_added,
        composer_lines_added,
        human_lines_added,
        ai_percentage,
    }
}
