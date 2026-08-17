//! 按来源把消耗记录切成 5 小时计费窗，并计算燃烧速率与窗末预测。
//! 只使用本地 `occurred_at`，不读官方配额、不访问网络。

use chrono::{DateTime, Duration, NaiveDateTime, SecondsFormat, Timelike, Utc};
use std::collections::{BTreeMap, BTreeSet};

use crate::cost::sum_costs;
use crate::domain::{
    BillingWindowDto, BillingWindowsDto, BurnRateDto, PriceTable, ProjectionDto, UsageRecord,
};

pub const WINDOW_HOURS: i64 = 5;
pub const LOOKBACK_DAYS: i64 = 14;
pub const RECENT_LIMIT: usize = 6;

struct Timed<'a> {
    at: DateTime<Utc>,
    record: &'a UsageRecord,
}

pub fn lookback_date(now: DateTime<Utc>) -> String {
    (now - Duration::days(LOOKBACK_DAYS))
        .format("%Y-%m-%d")
        .to_string()
}

pub fn summarize<'a, I>(records: I, prices: &PriceTable, now: DateTime<Utc>) -> BillingWindowsDto
where
    I: IntoIterator<Item = &'a UsageRecord>,
{
    let lookback = now - Duration::days(LOOKBACK_DAYS);
    let mut by_source: BTreeMap<String, Vec<Timed<'_>>> = BTreeMap::new();
    for record in records {
        let Some(at) = parse_occurred_at(&record.occurred_at) else {
            continue;
        };
        if at < lookback {
            continue;
        }
        by_source
            .entry(record.source.as_str().to_string())
            .or_default()
            .push(Timed { at, record });
    }

    let window_len = Duration::hours(WINDOW_HOURS);
    let mut current = Vec::new();
    let mut recent = Vec::new();
    for (_source, mut entries) in by_source {
        entries.sort_by_key(|entry| entry.at);
        for window in split_windows(&entries, now, window_len, prices) {
            if window.is_active {
                current.push(window);
            } else {
                recent.push(window);
            }
        }
    }

    current.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    recent.sort_by(|a, b| b.start.cmp(&a.start));
    recent.truncate(RECENT_LIMIT);

    BillingWindowsDto {
        now: iso(now),
        window_hours: WINDOW_HOURS,
        current,
        recent,
    }
}

fn split_windows(
    entries: &[Timed<'_>],
    now: DateTime<Utc>,
    window_len: Duration,
    prices: &PriceTable,
) -> Vec<BillingWindowDto> {
    if entries.is_empty() {
        return Vec::new();
    }

    let mut blocks: Vec<(DateTime<Utc>, Vec<&Timed<'_>>)> = Vec::new();
    let mut start = floor_to_utc_hour(entries[0].at);
    let mut current: Vec<&Timed<'_>> = Vec::new();

    for entry in entries {
        if let Some(last) = current.last() {
            if entry.at - start > window_len || entry.at - last.at > window_len {
                blocks.push((start, std::mem::take(&mut current)));
                start = floor_to_utc_hour(entry.at);
            }
        } else {
            start = floor_to_utc_hour(entry.at);
        }
        current.push(entry);
    }
    if !current.is_empty() {
        blocks.push((start, current));
    }

    blocks
        .into_iter()
        .map(|(start, items)| build_window(start, &items, now, window_len, prices))
        .collect()
}

fn build_window(
    start: DateTime<Utc>,
    items: &[&Timed<'_>],
    now: DateTime<Utc>,
    window_len: Duration,
    prices: &PriceTable,
) -> BillingWindowDto {
    let end = start + window_len;
    let first = items[0];
    let last = items[items.len() - 1];
    let last_activity = last.at;
    let is_active = now < end && now - last_activity < window_len;
    let records: Vec<&UsageRecord> = items.iter().map(|item| item.record).collect();
    let source = first.record.source;

    let mut total_tokens = 0;
    let mut input_tokens = 0;
    let mut output_tokens = 0;
    let mut cache_read_tokens = 0;
    let mut cache_creation_tokens = 0;
    let mut reasoning_tokens = 0;
    let mut sessions = BTreeSet::new();
    for record in &records {
        total_tokens += record.total_tokens;
        input_tokens += record.input_tokens;
        output_tokens += record.output_tokens;
        cache_read_tokens += record.cache_read_tokens;
        cache_creation_tokens += record.cache_creation_tokens;
        reasoning_tokens += record.reasoning_tokens;
        sessions.insert((record.source.as_str(), record.session_id.as_str()));
    }
    let (cost, unpriced) = sum_costs(&records, prices);
    let elapsed_minutes = if is_active {
        (now - start).num_minutes().max(0)
    } else {
        (std::cmp::min(now, end) - start).num_minutes().max(0)
    };
    let remaining_minutes = if is_active {
        Some((end - now).num_minutes().max(0))
    } else {
        None
    };
    let burn = burn_rate(first.at, last_activity, total_tokens, cost);
    let projection = if is_active {
        match (burn.as_ref(), remaining_minutes) {
            (Some(rate), Some(remaining)) => {
                Some(project_usage(total_tokens, cost, rate, remaining as f64))
            }
            _ => None,
        }
    } else {
        None
    };

    BillingWindowDto {
        source: source.as_str().to_string(),
        application: source.application_name().to_string(),
        start: iso(start),
        end: iso(end),
        last_activity: iso(last_activity),
        is_active,
        elapsed_minutes,
        remaining_minutes,
        total_tokens,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        reasoning_tokens,
        session_count: sessions.len() as i64,
        cost,
        unpriced,
        burn,
        projection,
    }
}

fn burn_rate(
    first: DateTime<Utc>,
    last: DateTime<Utc>,
    total_tokens: i64,
    cost: Option<f64>,
) -> Option<BurnRateDto> {
    let duration_minutes = (last - first).num_seconds() as f64 / 60.0;
    if duration_minutes <= 0.0 {
        return None;
    }
    Some(BurnRateDto {
        tokens_per_minute: total_tokens as f64 / duration_minutes,
        cost_per_hour: cost.map(|amount| amount / duration_minutes * 60.0),
    })
}

fn project_usage(
    used_tokens: i64,
    used_cost: Option<f64>,
    burn: &BurnRateDto,
    remaining_minutes: f64,
) -> ProjectionDto {
    ProjectionDto {
        total_tokens: (used_tokens as f64 + burn.tokens_per_minute * remaining_minutes).round()
            as i64,
        cost: match (used_cost, burn.cost_per_hour) {
            (Some(amount), Some(hourly)) => {
                Some(((amount + hourly / 60.0 * remaining_minutes) * 100.0).round() / 100.0)
            }
            _ => None,
        },
    }
}

pub fn parse_occurred_at(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value, format) {
            return Some(naive.and_utc());
        }
    }
    None
}

fn floor_to_utc_hour(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    timestamp
        .with_minute(0)
        .and_then(|value| value.with_second(0))
        .and_then(|value| value.with_nanosecond(0))
        .unwrap_or(timestamp)
}

fn iso(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Secs, true)
}
