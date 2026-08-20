//! 单日工作时间线：把当天各会话画成时间轴片段，供前端泳道布局渲染。
//! 只用已归一的消耗记录字段（项目名/来源/模型），不解析对话正文（见 CONTEXT.md）。
//! 会话区间 = 传入记录里能看到的该会话 occurred_at 范围，裁剪到本地日历日 `day` 后展示；
//! token 归属按每条记录 occurred_at 是否落在这天判定，跨午夜的会话只统计落在当天的部分。

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Local, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};

use crate::aggregate::assign_latest;
use crate::billing_window::parse_occurred_at;
use crate::domain::{UsageRecord, WorkSegment, WorkTimelineDto};

/// 给 SQL 层用的宽口径日期边界（前一天 ~ 后一天），覆盖本地时区可能造成的 ±1 天偏移；
/// 精确裁剪仍在 `build` 里按本地日历日判定，这里只是避免全表扫描的粗筛。
pub fn broad_date_bounds(day: &str) -> Option<(String, String)> {
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    Some((
        (date - Duration::days(1)).format("%Y-%m-%d").to_string(),
        (date + Duration::days(1)).format("%Y-%m-%d").to_string(),
    ))
}

struct SessionAcc {
    source: String,
    session_id: String,
    project: String,
    project_at: Option<String>,
    model: String,
    model_at: Option<String>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    day_tokens: i64,
}

/// 构建单日工作时间线。`records` 只需覆盖到各会话在 `day` 附近的记录（调用方可用
/// `broad_date_bounds` 粗筛再查询）；会话区间基于传入记录里能看到的 occurred_at 范围，
/// 不会去追溯该会话在此范围之外的历史，实践中足以覆盖跨午夜场景。
pub fn build(records: &[UsageRecord], day: &str) -> WorkTimelineDto {
    let Some(date) = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok() else {
        return WorkTimelineDto::empty(day);
    };
    let Some(midnight) = date.and_hms_opt(0, 0, 0) else {
        return WorkTimelineDto::empty(day);
    };
    let day_start = local_midnight_to_utc(midnight);
    let day_end = day_start + Duration::days(1);

    let mut sessions: BTreeMap<(String, String), SessionAcc> = BTreeMap::new();
    for record in records {
        let Some(at) = parse_occurred_at(&record.occurred_at) else {
            continue;
        };
        let key = (
            record.source.as_str().to_string(),
            record.session_id.clone(),
        );
        let entry = sessions.entry(key.clone()).or_insert_with(|| SessionAcc {
            source: key.0,
            session_id: key.1,
            project: String::new(),
            project_at: None,
            model: String::new(),
            model_at: None,
            start: at,
            end: at,
            day_tokens: 0,
        });
        if at < entry.start {
            entry.start = at;
        }
        if at > entry.end {
            entry.end = at;
        }
        assign_latest(
            &mut entry.project,
            &mut entry.project_at,
            &record.project,
            &record.occurred_at,
        );
        assign_latest(
            &mut entry.model,
            &mut entry.model_at,
            &record.model,
            &record.occurred_at,
        );
        if at >= day_start && at < day_end {
            entry.day_tokens += record.total_tokens;
        }
    }

    let mut segments: Vec<WorkSegment> = sessions
        .into_values()
        // 工作片段数 = 会话区间与当天区间有交集，不要求该会话在这天有具体的一条记录。
        .filter(|acc| acc.start < day_end && acc.end >= day_start)
        .map(|acc| {
            let clip_start = acc.start.max(day_start);
            let clip_end = acc.end.min(day_end).max(clip_start);
            WorkSegment {
                session_id: acc.session_id,
                source: acc.source,
                project: acc.project,
                model: acc.model,
                start: iso(clip_start),
                end: iso(clip_end),
                total_tokens: acc.day_tokens,
            }
        })
        .collect();
    segments.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });

    let total_tokens: i64 = segments.iter().map(|segment| segment.total_tokens).sum();
    let segment_count = segments.len() as i64;

    WorkTimelineDto {
        day: day.to_string(),
        total_tokens,
        segment_count,
        segments,
    }
}

/// 本地日历日零点 -> UTC 时刻。夏令时切换缺失该本地时刻的极端情况下退化为按 UTC 处理。
fn local_midnight_to_utc(naive: NaiveDateTime) -> DateTime<Utc> {
    match naive.and_local_timezone(Local) {
        LocalResult::Single(dt) => dt.with_timezone(&Utc),
        LocalResult::Ambiguous(earliest, _) => earliest.with_timezone(&Utc),
        LocalResult::None => Utc.from_utc_datetime(&naive),
    }
}

fn iso(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
