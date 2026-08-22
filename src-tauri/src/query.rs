//! SQL 下推的聚合查询：把原先「load_all 全量载入内存再聚合」改为在 sqlite 里
//! GROUP BY / 过滤，只返回聚合结果。费用通过临时价格表 `price_rows` LEFT JOIN 计算，
//! 与 `cost::derive_cost` 保持同一语义（native_cost 优先，其次 model+provider 匹配，
//! 再次 model 且 provider 为 NULL 的兜底，都没有则标记 unpriced；model/provider 大小写不敏感）。

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rusqlite::{params, params_from_iter, types::Value, Connection, Row};

use crate::billing_window;
use crate::cursor_account;
use crate::domain::{
    ApplicationAnalyticsDto, ApplicationEfficiency, ApplicationTrendPoint, BillingWindowsDto,
    CostSource, EfficiencyMetrics, Filter, FilterOptions, InstructionSourceUsage,
    InstructionUsageSummary, NamedAmount, OverviewDto, PriceTable, ProjectApplicationRow,
    SeriesPoint, SessionPage, SessionQuery, SessionRow, Source, TurnRow, UsageRecord,
    WorkTimelineDto,
};

/// 费用表达式（每行）：native_cost 优先，否则加权价格，否则 NULL（未定价）。
const COST_EXPR: &str = "
    CASE
        WHEN r.native_cost IS NOT NULL THEN r.native_cost
        WHEN COALESCE(pe.input, pf.input) IS NOT NULL THEN
            COALESCE(pe.input, pf.input) * r.input_tokens
            + COALESCE(pe.output, pf.output) * r.output_tokens
            + COALESCE(pe.cache_read, pf.cache_read) * r.cache_read_tokens
            + COALESCE(pe.cache_creation, pf.cache_creation) * r.cache_creation_tokens
        ELSE NULL
    END";

/// 未定价标志（每行 0/1）。
const UNPRICED_EXPR: &str = "
    CASE
        WHEN r.native_cost IS NOT NULL THEN 0
        WHEN COALESCE(pe.input, pf.input) IS NOT NULL THEN 0
        ELSE 1
    END";

/// 费用来源：native > 精确匹配条目 origin > 兜底条目 origin > none。
const COST_SOURCE_EXPR: &str = "
    CASE
        WHEN r.native_cost IS NOT NULL THEN 'native'
        WHEN pe.model IS NOT NULL THEN COALESCE(pe.origin, 'user')
        WHEN pf.model IS NOT NULL THEN COALESCE(pf.origin, 'user')
        ELSE 'none'
    END";

/// 价格表两次 LEFT JOIN：pe 匹配 model+provider，pf 兜底 model 且 provider 为空。
/// 键在 `install_prices` 里已折成 ASCII 小写，与 `cost::model_matches` 一致。
///
/// `r.model` 不套 `lower()`：`store::insert_records` 写入时已归一化，`migrate_lowercase_model`
/// 也补齐了历史数据，两边同口径。对全表逐行调函数会让 17 万行各多付两次调用。
/// `r.provider` 仍要 `lower()`——历史值里有 `cpaApi` 这类混合大小写，归一化会改到界面显示。
const PRICE_JOINS: &str = "
    LEFT JOIN price_rows pe ON pe.model = r.model AND pe.provider = lower(r.provider)
    LEFT JOIN price_rows pf ON pf.model = r.model AND pf.provider IS NULL";

/// 把价目表装进临时表 `price_rows`，供 `PRICE_JOINS` 取价。
///
/// 每次查询重建一遍看着浪费，实测却只要 1.7ms——SQLite 在单个事务里插 1400 行就是这么快。
/// 曾经按指纹跳过重建，收益不到首屏的 1%，不值当缓存失效那份复杂度，已经撤掉。
/// 真正的开销在 `PRICE_JOINS` 逐行取价那边，那个换不掉：先按 (model, provider) 聚合再算价
/// 数学上等价，但 GROUP BY 比 JOIN 更贵，实测反而慢。
fn install_prices(conn: &Connection, prices: &PriceTable) -> Result<(), String> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS price_rows;
         CREATE TEMP TABLE price_rows (
             model TEXT NOT NULL,
             provider TEXT,
             input REAL NOT NULL DEFAULT 0,
             output REAL NOT NULL DEFAULT 0,
             cache_read REAL NOT NULL DEFAULT 0,
             cache_creation REAL NOT NULL DEFAULT 0,
             origin TEXT NOT NULL DEFAULT 'user'
         );
         CREATE INDEX price_rows_model_provider ON price_rows(model, provider);",
    )
    .map_err(|e| e.to_string())?;
    if prices.prices.is_empty() {
        return Ok(());
    }
    let mut stmt = conn
        .prepare(
            "INSERT INTO price_rows (model, provider, input, output, cache_read, cache_creation, origin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .map_err(|e| e.to_string())?;
    for entry in &prices.prices {
        stmt.execute(params![
            entry.model.to_ascii_lowercase(),
            entry
                .provider
                .as_ref()
                .map(|value| value.to_ascii_lowercase()),
            entry.input,
            entry.output,
            entry.cache_read,
            entry.cache_creation,
            entry.origin.as_str(),
        ])
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Filter → (WHERE 子句片段列表, 参数)。所有列都加 `r.` 前缀（表别名 r）。
fn filter_clauses(filter: &Filter) -> (Vec<String>, Vec<Value>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut params: Vec<Value> = Vec::new();
    if let Some(from) = &filter.from {
        clauses.push("r.occurred_at >= ?".to_string());
        params.push(Value::Text(from.clone()));
    }
    if let Some(to) = &filter.to {
        clauses.push("r.occurred_at <= ?".to_string());
        params.push(Value::Text(to.clone()));
    }
    if !filter.sources.is_empty() {
        clauses.push(format!(
            "r.source IN ({})",
            placeholders(filter.sources.len())
        ));
        for s in &filter.sources {
            params.push(Value::Text(s.clone()));
        }
    }
    if !filter.models.is_empty() {
        clauses.push(format!(
            "r.model IN ({})",
            placeholders(filter.models.len())
        ));
        for m in &filter.models {
            params.push(Value::Text(m.clone()));
        }
    }
    if !filter.projects.is_empty() {
        clauses.push(format!(
            "r.project IN ({})",
            placeholders(filter.projects.len())
        ));
        for p in &filter.projects {
            params.push(Value::Text(p.clone()));
        }
    }
    if !filter.providers.is_empty() {
        clauses.push(format!(
            "r.provider IN ({})",
            placeholders(filter.providers.len())
        ));
        for p in &filter.providers {
            params.push(Value::Text(p.clone()));
        }
    }
    (clauses, params)
}

fn placeholders(n: usize) -> String {
    std::iter::repeat_n("?", n).collect::<Vec<_>>().join(", ")
}

pub(crate) struct SessionUsageTotals {
    pub total_tokens: i64,
    pub cost: Option<f64>,
    pub unpriced: bool,
}

/// 按精确 `(source, session_id)` 聚合消耗记录。对话目录挂用量，不改变会话管理口径。
pub(crate) fn usage_rollups_for_sessions(
    conn: &Connection,
    prices: &PriceTable,
    keys: &[(String, String)],
) -> Result<BTreeMap<(String, String), SessionUsageTotals>, String> {
    let mut totals = BTreeMap::new();
    if keys.is_empty() {
        return Ok(totals);
    }
    install_prices(conn, prices)?;
    let mut clauses = Vec::with_capacity(keys.len());
    let mut params: Vec<Value> = Vec::with_capacity(keys.len() * 2);
    for (source, session_id) in keys {
        clauses.push("(r.source = ? AND r.session_id = ?)".to_string());
        params.push(Value::Text(source.clone()));
        params.push(Value::Text(session_id.clone()));
    }
    let sql = format!(
        "SELECT r.source, r.session_id,
            COALESCE(SUM(r.total_tokens), 0),
            SUM({COST_EXPR}),
            COALESCE(SUM({UNPRICED_EXPR}), 0)
         FROM usage_records r
         {PRICE_JOINS}
         WHERE {}
         GROUP BY r.source, r.session_id",
        clauses.join(" OR "),
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<f64>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (source, session_id, total_tokens, cost, unpriced_count) in rows {
        totals.insert(
            (source, session_id),
            SessionUsageTotals {
                total_tokens,
                cost,
                unpriced: unpriced_count > 0,
            },
        );
    }
    Ok(totals)
}

/// 转义 LIKE 通配符，避免用户输入的 `%`/`_` 被解释为通配符。
fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn where_sql(clauses: &[String]) -> String {
    if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    }
}

/// 时间桶表达式（hour/day/week/month）。occurred_at 为 ISO 文本，前缀截取即对应粒度。
fn bucket_expr(grain: &str) -> &'static str {
    match grain {
        "hour" => "substr(r.occurred_at, 1, 13)",
        "week" => "strftime('%G-W%V', substr(r.occurred_at, 1, 10))",
        "month" => "substr(r.occurred_at, 1, 7)",
        _ => "substr(r.occurred_at, 1, 10)",
    }
}

fn ratio(numerator: i64, denominator: i64) -> Option<f64> {
    if denominator <= 0 {
        None
    } else {
        Some(numerator as f64 / denominator as f64)
    }
}

pub fn overview(
    conn: &Connection,
    filter: &Filter,
    prices: &PriceTable,
) -> Result<OverviewDto, String> {
    install_prices(conn, prices)?;
    let (clauses, params) = filter_clauses(filter);
    let sql = format!(
        "SELECT
            COALESCE(SUM(r.total_tokens), 0),
            COALESCE(SUM(r.input_tokens), 0),
            COALESCE(SUM(r.output_tokens), 0),
            COALESCE(SUM(r.cache_read_tokens), 0),
            COALESCE(SUM(r.cache_creation_tokens), 0),
            COALESCE(SUM(r.reasoning_tokens), 0),
            COUNT(DISTINCT r.source || char(31) || r.session_id),
            SUM({COST_EXPR}),
            COALESCE(SUM({UNPRICED_EXPR}), 0)
        FROM usage_records r
        {PRICE_JOINS}
        {}",
        where_sql(&clauses),
    );
    conn.query_row(&sql, params_from_iter(params.iter()), |row| {
        Ok(OverviewDto {
            total_tokens: row.get(0)?,
            input_tokens: row.get(1)?,
            output_tokens: row.get(2)?,
            cache_read_tokens: row.get(3)?,
            cache_creation_tokens: row.get(4)?,
            reasoning_tokens: row.get(5)?,
            session_count: row.get(6)?,
            cost: row.get(7)?,
            unpriced: row.get::<_, i64>(8)? > 0,
        })
    })
    .map_err(|e| e.to_string())
}

/// 全时段、全来源的费用标量。给代码量 ROI 用，不扫 token 维度、不算会话数。
pub fn lifetime_cost(
    conn: &Connection,
    prices: &PriceTable,
) -> Result<(Option<f64>, bool), String> {
    install_prices(conn, prices)?;
    let sql = format!(
        "SELECT SUM({COST_EXPR}), COALESCE(SUM({UNPRICED_EXPR}), 0)
         FROM usage_records r
         {PRICE_JOINS}"
    );
    conn.query_row(&sql, [], |row| Ok((row.get(0)?, row.get::<_, i64>(1)? > 0)))
        .map_err(|e| e.to_string())
}

/// `billing_windows` 与 `work_timeline` 宽口径拉取共用的列清单，列序与 `usage_record_from_row` 一一对应。
const USAGE_RECORD_COLUMNS: &str =
    "r.occurred_at, r.source, r.model, r.provider, r.project, r.session_id, r.source_file,
    r.input_tokens, r.output_tokens, r.cache_read_tokens, r.cache_creation_tokens,
    r.reasoning_tokens, r.total_tokens, r.native_cost";

/// 把 `USAGE_RECORD_COLUMNS` 那 14 列（固定列序）映射回 `UsageRecord`。
fn usage_record_from_row(row: &Row) -> rusqlite::Result<UsageRecord> {
    let source_value: String = row.get(1)?;
    let source = Source::parse(&source_value).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            format!("未知来源：{source_value}").into(),
        )
    })?;
    Ok(UsageRecord {
        occurred_at: row.get(0)?,
        source,
        model: row.get(2)?,
        provider: row.get(3)?,
        project: row.get(4)?,
        session_id: row.get(5)?,
        source_file: row.get(6)?,
        input_tokens: row.get(7)?,
        output_tokens: row.get(8)?,
        cache_read_tokens: row.get(9)?,
        cache_creation_tokens: row.get(10)?,
        reasoning_tokens: row.get(11)?,
        total_tokens: row.get(12)?,
        native_cost: row.get(13)?,
    })
}

pub fn billing_windows(
    conn: &Connection,
    filter: &Filter,
    prices: &PriceTable,
    now: DateTime<Utc>,
) -> Result<BillingWindowsDto, String> {
    let scoped = Filter {
        from: None,
        to: None,
        sources: filter.sources.clone(),
        models: filter.models.clone(),
        projects: filter.projects.clone(),
        providers: filter.providers.clone(),
    };
    let (mut clauses, mut params) = filter_clauses(&scoped);
    clauses.push("substr(r.occurred_at, 1, 10) >= ?".to_string());
    params.push(Value::Text(billing_window::lookback_date(now)));
    let sql = format!(
        "SELECT {USAGE_RECORD_COLUMNS}
        FROM usage_records r
        {}
        ORDER BY r.occurred_at",
        where_sql(&clauses),
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(params.iter()), usage_record_from_row)
        .map_err(|e| e.to_string())?;
    let records = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let dto = billing_window::summarize(&records, prices, now);
    let cursor_events = cursor_account::events_for_weekly_window(conn, filter)?;
    Ok(billing_window::attach_cursor_weekly(
        dto,
        &cursor_events,
        prices,
        now,
    ))
}

pub fn trend(
    conn: &Connection,
    filter: &Filter,
    prices: &PriceTable,
    grain: &str,
) -> Result<Vec<SeriesPoint>, String> {
    install_prices(conn, prices)?;
    let bucket = bucket_expr(grain);
    let (clauses, params) = filter_clauses(filter);
    let sql = format!(
        "SELECT {bucket} AS bucket,
            SUM(r.total_tokens),
            SUM(r.input_tokens),
            SUM(r.output_tokens),
            SUM(r.cache_read_tokens),
            SUM(r.cache_creation_tokens),
            SUM(r.reasoning_tokens),
            SUM({COST_EXPR})
        FROM usage_records r
        {PRICE_JOINS}
        {}
        GROUP BY 1
        ORDER BY 1",
        where_sql(&clauses),
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok(SeriesPoint {
                bucket: row.get(0)?,
                total_tokens: row.get(1)?,
                input_tokens: row.get(2)?,
                output_tokens: row.get(3)?,
                cache_read_tokens: row.get(4)?,
                cache_creation_tokens: row.get(5)?,
                reasoning_tokens: row.get(6)?,
                cost: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn breakdown_name_expr(dimension: &str) -> Result<&'static str, String> {
    match dimension {
        "application" | "source" => Ok("r.source"),
        "model" => Ok("r.model"),
        "provider" => Ok("r.provider"),
        "project" => Ok("r.project"),
        _ => Err(format!("不支持的统计维度：{dimension}")),
    }
}

fn display_name(raw: &str, dimension: &str) -> String {
    if dimension == "application" {
        Source::parse(raw)
            .map(|s| s.application_name().to_string())
            .unwrap_or_else(|| raw.to_string())
    } else if raw.is_empty() {
        "（未标注）".to_string()
    } else {
        raw.to_string()
    }
}

pub fn breakdown(
    conn: &Connection,
    filter: &Filter,
    prices: &PriceTable,
    dimension: &str,
) -> Result<Vec<NamedAmount>, String> {
    install_prices(conn, prices)?;
    let name_expr = breakdown_name_expr(dimension)?;
    let (clauses, params) = filter_clauses(filter);
    let sql = format!(
        "SELECT {name_expr} AS name,
            SUM(r.total_tokens),
            SUM({COST_EXPR}),
            COALESCE(SUM({UNPRICED_EXPR}), 0)
        FROM usage_records r
        {PRICE_JOINS}
        {}
        GROUP BY 1",
        where_sql(&clauses),
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let raw = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let grand: i64 = raw.iter().map(|(_, total, _, _)| *total).sum();
    let mut rows: Vec<NamedAmount> = raw
        .into_iter()
        .map(|(name, total_tokens, cost, unpriced_count)| NamedAmount {
            name: display_name(&name, dimension),
            total_tokens,
            share: if grand == 0 {
                0.0
            } else {
                total_tokens as f64 / grand as f64
            },
            cost,
            unpriced: unpriced_count > 0,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(rows)
}

pub fn application_analytics(
    conn: &Connection,
    filter: &Filter,
    grain: &str,
) -> Result<ApplicationAnalyticsDto, String> {
    let (clauses, params) = filter_clauses(filter);
    let where_sql = where_sql(&clauses);

    let summary_sql = format!(
        "SELECT
            COALESCE(SUM(r.total_tokens), 0),
            COALESCE(SUM(r.input_tokens), 0),
            COALESCE(SUM(r.cache_read_tokens), 0),
            COALESCE(SUM(r.reasoning_tokens), 0),
            COUNT(DISTINCT r.source || char(31) || r.session_id)
        FROM usage_records r
        {where_sql}"
    );
    let (total, input, cache_read, reasoning, session_count) = conn
        .query_row(&summary_sql, params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let summary = EfficiencyMetrics {
        total_tokens: total,
        session_count,
        cache_hit_rate: ratio(cache_read, input + cache_read),
        average_session_tokens: if session_count == 0 {
            None
        } else {
            Some(total as f64 / session_count as f64)
        },
        reasoning_share: ratio(reasoning, total),
    };

    let app_sql = format!(
        "SELECT r.source,
            SUM(r.total_tokens),
            SUM(r.input_tokens),
            SUM(r.cache_read_tokens),
            SUM(r.reasoning_tokens),
            COUNT(DISTINCT r.session_id)
        FROM usage_records r
        {where_sql}
        GROUP BY r.source"
    );
    let mut stmt = conn.prepare(&app_sql).map_err(|e| e.to_string())?;
    let app_rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut by_application: Vec<ApplicationEfficiency> = app_rows
        .into_iter()
        .filter_map(
            |(source, total, input, cache_read, reasoning, session_count)| {
                let parsed = Source::parse(&source)?;
                Some(ApplicationEfficiency {
                    source,
                    application: parsed.application_name().to_string(),
                    metrics: EfficiencyMetrics {
                        total_tokens: total,
                        session_count,
                        cache_hit_rate: ratio(cache_read, input + cache_read),
                        average_session_tokens: if session_count == 0 {
                            None
                        } else {
                            Some(total as f64 / session_count as f64)
                        },
                        reasoning_share: ratio(reasoning, total),
                    },
                })
            },
        )
        .collect();
    by_application.sort_by(|a, b| {
        b.metrics
            .total_tokens
            .cmp(&a.metrics.total_tokens)
            .then_with(|| a.application.cmp(&b.application))
    });

    let bucket = bucket_expr(grain);
    let trend_sql = format!(
        "SELECT {bucket} AS bucket, r.source, SUM(r.total_tokens)
        FROM usage_records r
        {where_sql}
        GROUP BY 1, 2
        ORDER BY 1, 2"
    );
    let mut stmt = conn.prepare(&trend_sql).map_err(|e| e.to_string())?;
    let trend_rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut trend_map: BTreeMap<String, ApplicationTrendPoint> = BTreeMap::new();
    for (bucket, source, total) in trend_rows {
        let point = trend_map
            .entry(bucket.clone())
            .or_insert_with(|| ApplicationTrendPoint {
                bucket,
                total_tokens: 0,
                values: BTreeMap::new(),
            });
        point.total_tokens += total;
        *point.values.entry(source).or_default() += total;
    }

    let project_sql = format!(
        "SELECT r.project, r.source, SUM(r.total_tokens)
        FROM usage_records r
        {where_sql}
        GROUP BY 1, 2
        ORDER BY 1, 2"
    );
    let mut stmt = conn.prepare(&project_sql).map_err(|e| e.to_string())?;
    let project_rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut projects_map: BTreeMap<String, ProjectApplicationRow> = BTreeMap::new();
    for (project, source, total) in project_rows {
        let project = if project.is_empty() {
            "（未标注）".to_string()
        } else {
            project
        };
        let row = projects_map
            .entry(project.clone())
            .or_insert_with(|| ProjectApplicationRow {
                project,
                total_tokens: 0,
                values: BTreeMap::new(),
            });
        row.total_tokens += total;
        *row.values.entry(source).or_default() += total;
    }
    let mut projects: Vec<ProjectApplicationRow> = projects_map.into_values().collect();
    projects.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| a.project.cmp(&b.project))
    });

    Ok(ApplicationAnalyticsDto {
        summary,
        by_application,
        trend: trend_map.into_values().collect(),
        projects,
    })
}

pub fn top_sessions(
    conn: &Connection,
    filter: &Filter,
    prices: &PriceTable,
    limit: usize,
) -> Result<Vec<SessionRow>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    install_prices(conn, prices)?;
    let (clauses, mut params) = filter_clauses(filter);
    // 先按 token 取出 Top N。相关子查询不能放进全表 GROUP BY：
    // 17 万行 × 3 次会话回表会把首屏卡死。
    let sql = format!(
        "SELECT r.source, r.session_id,
            SUM(r.total_tokens),
            MIN(r.occurred_at),
            MAX(r.occurred_at),
            SUM({COST_EXPR}),
            COALESCE(SUM({UNPRICED_EXPR}), 0)
        FROM usage_records r
        {PRICE_JOINS}
        {}
        GROUP BY r.source, r.session_id
        ORDER BY SUM(r.total_tokens) DESC, r.source ASC, r.session_id ASC
        LIMIT ?",
        where_sql(&clauses),
    );
    params.push(Value::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let raw = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<f64>>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut rows: Vec<SessionRow> = raw
        .into_iter()
        .map(
            |(source, session_id, total_tokens, started_at, ended_at, cost, unpriced_count)| {
                SessionRow {
                    session_id,
                    source,
                    project: String::new(),
                    model: String::new(),
                    total_tokens,
                    started_at,
                    ended_at,
                    source_file: String::new(),
                    cost,
                    unpriced: unpriced_count > 0,
                }
            },
        )
        .collect();
    hydrate_session_labels(conn, &mut rows)?;
    Ok(rows)
}

/// 回表补齐 top N 会话的展示标签（项目 / 模型 / 原始文件）。
///
/// 与 `session_rollup_sql` 同一套「一次扫描取最晚非空值」写法：内层 GROUP BY 聚出
/// `occurred_at || sep || value` 的 MAX，外层再切出值。早先这里用的是三个相关子查询，
/// 每个会话每列都要把该会话的全部行按 occurred_at 排一遍——首屏 top 8 会话合计 6.2 万行时
/// 实测 1.03s，换成一次扫描后 304ms。
fn hydrate_session_labels(conn: &Connection, rows: &mut [SessionRow]) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut clauses = Vec::with_capacity(rows.len());
    let mut params: Vec<Value> = Vec::with_capacity(rows.len() * 2);
    for row in rows.iter() {
        clauses.push("(r.source = ? AND r.session_id = ?)".to_string());
        params.push(Value::Text(row.source.clone()));
        params.push(Value::Text(row.session_id.clone()));
    }
    let sql = format!(
        "SELECT source, session_id, {} AS project, {} AS model, {} AS source_file
         FROM (
            SELECT r.source AS source, r.session_id AS session_id,
                {} AS project_key,
                {} AS model_key,
                {} AS file_key
            FROM usage_records r
            WHERE {}
            GROUP BY r.source, r.session_id
         )",
        unwrap_latest_key_sql("project_key"),
        unwrap_latest_key_sql("model_key"),
        unwrap_latest_key_sql("file_key"),
        latest_nonempty_key_sql("project"),
        latest_nonempty_key_sql("model"),
        latest_nonempty_key_sql("source_file"),
        clauses.join(" OR "),
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let labels = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (source, session_id, project, model, source_file) in labels {
        if let Some(row) = rows
            .iter_mut()
            .find(|row| row.source == source && row.session_id == session_id)
        {
            row.project = project;
            row.model = model;
            row.source_file = source_file;
        }
    }
    Ok(())
}

/// 一次扫描取出「最晚非空」键：`MAX(occurred_at || sep || value)` 与
/// `ORDER BY occurred_at DESC, value DESC LIMIT 1` 同序。
fn latest_nonempty_key_sql(column: &str) -> String {
    format!("MAX(CASE WHEN r.{column} != '' THEN r.occurred_at || char(31) || r.{column} END)")
}

fn unwrap_latest_key_sql(alias: &str) -> String {
    format!("COALESCE(substr({alias}, instr({alias}, char(31)) + 1), '')")
}

fn session_rollup_sql(clauses: &[String], include_cost: bool) -> String {
    let project_key = latest_nonempty_key_sql("project");
    let model_key = latest_nonempty_key_sql("model");
    let file_key = latest_nonempty_key_sql("source_file");
    let project = unwrap_latest_key_sql("project_key");
    let model = unwrap_latest_key_sql("model_key");
    let source_file = unwrap_latest_key_sql("file_key");
    let (cost_select, joins) = if include_cost {
        (
            format!(
                "SUM({COST_EXPR}) AS cost, COALESCE(SUM({UNPRICED_EXPR}), 0) AS unpriced_count"
            ),
            PRICE_JOINS,
        )
    } else {
        (
            "CAST(NULL AS REAL) AS cost, 0 AS unpriced_count".to_string(),
            "",
        )
    };
    format!(
        "SELECT source, session_id, total_tokens, started_at, ended_at,
            {project} AS project,
            {model} AS model,
            {source_file} AS source_file,
            cost, unpriced_count
         FROM (
            SELECT r.source AS source, r.session_id AS session_id,
                SUM(r.total_tokens) AS total_tokens,
                MIN(r.occurred_at) AS started_at,
                MAX(r.occurred_at) AS ended_at,
                {project_key} AS project_key,
                {model_key} AS model_key,
                {file_key} AS file_key,
                {cost_select}
            FROM usage_records r
            {joins}
            {}
            GROUP BY r.source, r.session_id
         )",
        where_sql(clauses),
    )
}

/// 会话列表的分页查询：搜索（session/项目/模型/应用/原始文件）、排序、分页均在 SQL 层完成。
/// 汇总与当前页共用一次 MATERIALIZED 聚合，避免对消耗记录扫两遍。
pub fn sessions_page(
    conn: &Connection,
    prices: &PriceTable,
    query: &SessionQuery,
) -> Result<SessionPage, String> {
    let include_cost = query.include_cost.unwrap_or(false);
    if include_cost {
        install_prices(conn, prices)?;
    }
    let (clauses, mut params) = filter_clauses(&query.filter);
    let sessions_cte = session_rollup_sql(&clauses, include_cost);

    let search_clause = match query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(search) => {
            let pattern = format!("%{}%", escape_like(search));
            for _ in 0..5 {
                params.push(Value::Text(pattern.clone()));
            }
            "WHERE (session_id LIKE ? ESCAPE '\\' OR project LIKE ? ESCAPE '\\'
                OR model LIKE ? ESCAPE '\\' OR source LIKE ? ESCAPE '\\'
                OR source_file LIKE ? ESCAPE '\\')"
                .to_string()
        }
        None => String::new(),
    };

    let sort_column = match query.sort_by.as_deref() {
        Some("session") => "session_id",
        Some("application") => "source",
        Some("project") => "project",
        Some("model") => "model",
        Some("cost") => "cost",
        Some("time") => "ended_at",
        _ => "total_tokens",
    };
    let sort_dir = if query.sort_dir.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 20_000);
    let offset = (page - 1) * page_size;
    params.push(Value::Integer(page_size as i64));
    params.push(Value::Integer(offset as i64));

    let sql = format!(
        "WITH sessions AS MATERIALIZED ({sessions_cte}),
            filtered AS MATERIALIZED (
                SELECT * FROM sessions {search_clause}
            ),
            summary AS (
                SELECT COUNT(*) AS match_count,
                    COALESCE(SUM(total_tokens), 0) AS match_tokens,
                    MAX(ended_at) AS match_last_ended
                FROM filtered
            ),
            page AS (
                SELECT session_id, source, project, model, total_tokens, started_at, ended_at,
                    source_file, cost, unpriced_count
                FROM filtered
                ORDER BY {sort_column} {sort_dir}, session_id ASC
                LIMIT ? OFFSET ?
            )
         SELECT summary.match_count, summary.match_tokens, summary.match_last_ended,
            page.session_id, page.source, page.project, page.model, page.total_tokens,
            page.started_at, page.ended_at, page.source_file, page.cost, page.unpriced_count
         FROM summary
         LEFT JOIN page ON 1"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let raw = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<f64>>(11)?,
                row.get::<_, Option<i64>>(12)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut total = 0;
    let mut total_tokens = 0;
    let mut last_ended = None;
    let mut rows = Vec::new();
    for (
        match_count,
        match_tokens,
        match_last_ended,
        session_id,
        source,
        project,
        model,
        row_tokens,
        started_at,
        ended_at,
        source_file,
        cost,
        unpriced_count,
    ) in raw
    {
        total = match_count;
        total_tokens = match_tokens;
        last_ended = match_last_ended;
        let Some(session_id) = session_id else {
            continue;
        };
        rows.push(SessionRow {
            session_id,
            source: source.unwrap_or_default(),
            project: project.unwrap_or_default(),
            model: model.unwrap_or_default(),
            total_tokens: row_tokens.unwrap_or(0),
            started_at: started_at.unwrap_or_default(),
            ended_at: ended_at.unwrap_or_default(),
            source_file: source_file.unwrap_or_default(),
            cost,
            unpriced: unpriced_count.unwrap_or(0) > 0,
        });
    }

    Ok(SessionPage {
        rows,
        total,
        total_tokens,
        last_ended,
    })
}

pub fn session_turns(
    conn: &Connection,
    session_id: &str,
    source: Option<&str>,
    filter: &Filter,
    prices: &PriceTable,
) -> Result<Vec<TurnRow>, String> {
    install_prices(conn, prices)?;
    let (mut clauses, mut params) = filter_clauses(filter);
    clauses.push("r.session_id = ?".to_string());
    params.push(Value::Text(session_id.to_string()));
    if let Some(source) = source {
        clauses.push("r.source = ?".to_string());
        params.push(Value::Text(source.to_string()));
    }
    let sql = format!(
        "SELECT r.occurred_at, r.model, r.provider,
            r.input_tokens, r.output_tokens, r.cache_read_tokens, r.cache_creation_tokens,
            r.reasoning_tokens, r.total_tokens, r.source_file,
            {COST_EXPR},
            {UNPRICED_EXPR},
            {COST_SOURCE_EXPR}
        FROM usage_records r
        {PRICE_JOINS}
        {}
        ORDER BY r.occurred_at",
        where_sql(&clauses),
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            let cost: Option<f64> = row.get(10)?;
            let unpriced: i64 = row.get(11)?;
            let cost_source = CostSource::from_sql(row.get::<_, String>(12)?.as_str());
            Ok(TurnRow {
                occurred_at: row.get(0)?,
                model: row.get(1)?,
                provider: row.get(2)?,
                input_tokens: row.get(3)?,
                output_tokens: row.get(4)?,
                cache_read_tokens: row.get(5)?,
                cache_creation_tokens: row.get(6)?,
                reasoning_tokens: row.get(7)?,
                total_tokens: row.get(8)?,
                source_file: row.get(9)?,
                cost,
                unpriced: unpriced > 0,
                cost_source,
                cost_note: Some(cost_source.note().to_string()),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// 单日工作时间线：宽口径拉取 `day` 前后各一天的记录（覆盖本地时区可能造成的偏移），
/// 精确的当天裁剪与聚合交给 `crate::work_timeline::build`——与内存路径 `aggregate::work_timeline`
/// 共用同一份逻辑，由 `tests/parity.rs` 保证两条路径结果一致。
pub fn work_timeline(conn: &Connection, day: &str) -> Result<WorkTimelineDto, String> {
    let Some((from, to)) = crate::work_timeline::broad_date_bounds(day) else {
        return Ok(WorkTimelineDto::empty(day));
    };
    let sql = format!(
        "SELECT {USAGE_RECORD_COLUMNS}
        FROM usage_records r
        WHERE substr(r.occurred_at, 1, 10) >= ?1 AND substr(r.occurred_at, 1, 10) <= ?2
        ORDER BY r.occurred_at"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![from, to], usage_record_from_row)
        .map_err(|e| e.to_string())?;
    let records = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(crate::work_timeline::build(&records, day))
}

pub fn filter_options(conn: &Connection) -> Result<FilterOptions, String> {
    fn distinct(conn: &Connection, sql: &str) -> Result<Vec<String>, String> {
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
    Ok(FilterOptions {
        sources: distinct(
            conn,
            "SELECT DISTINCT source FROM usage_records ORDER BY source",
        )?,
        models: distinct(
            conn,
            "SELECT DISTINCT model FROM usage_records WHERE model != '' ORDER BY model",
        )?,
        projects: distinct(
            conn,
            "SELECT DISTINCT project FROM usage_records WHERE project != '' ORDER BY project",
        )?,
        providers: distinct(
            conn,
            "SELECT DISTINCT provider FROM usage_records WHERE provider != '' ORDER BY provider",
        )?,
    })
}

pub fn recent_projects(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT project FROM usage_records
             WHERE project != ''
             GROUP BY project
             ORDER BY MAX(occurred_at) DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn source_token_totals(conn: &Connection) -> Result<InstructionUsageSummary, String> {
    let mut stmt = conn
        .prepare(
            "SELECT source, SUM(total_tokens) FROM usage_records
             GROUP BY source
             ORDER BY SUM(total_tokens) DESC, source ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(InstructionSourceUsage {
                source: row.get(0)?,
                total_tokens: row.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(InstructionUsageSummary {
        sources: rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?,
    })
}
