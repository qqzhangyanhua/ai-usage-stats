use crate::domain::{
    CostSource, CursorUsageEvent, DerivedCost, PriceOrigin, PriceTable, UsageRecord,
};

struct PricedTokens<'a> {
    model: &'a str,
    provider: &'a str,
    input_tokens: i64,
    output_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_tokens: i64,
    native_cost: Option<f64>,
}

pub fn derive_cost(record: &UsageRecord, prices: &PriceTable) -> DerivedCost {
    derive_priced(
        PricedTokens {
            model: &record.model,
            provider: &record.provider,
            input_tokens: record.input_tokens,
            output_tokens: record.output_tokens,
            cache_read_tokens: record.cache_read_tokens,
            cache_creation_tokens: record.cache_creation_tokens,
            native_cost: record.native_cost,
        },
        prices,
    )
}

/// 按模型计价：native_cost 优先，其次用户价目，再次 LiteLLM 快照（provider 为空的兜底）。
fn derive_priced(usage: PricedTokens<'_>, prices: &PriceTable) -> DerivedCost {
    if let Some(amount) = usage.native_cost {
        return DerivedCost {
            amount: Some(amount),
            unpriced: false,
            source_native: true,
            cost_source: CostSource::Native,
        };
    }
    if let Some(entry) = find_price(usage.model, usage.provider, prices) {
        let amount = (usage.input_tokens as f64) * entry.input
            + (usage.output_tokens as f64) * entry.output
            + (usage.cache_read_tokens as f64) * entry.cache_read
            + (usage.cache_creation_tokens as f64) * entry.cache_creation;
        let cost_source = match entry.origin {
            PriceOrigin::Snapshot => CostSource::Snapshot,
            PriceOrigin::User => CostSource::User,
        };
        return DerivedCost {
            amount: Some(amount),
            unpriced: false,
            source_native: false,
            cost_source,
        };
    }
    DerivedCost {
        amount: None,
        unpriced: true,
        source_native: false,
        cost_source: CostSource::None,
    }
}

fn find_price<'a>(
    model: &str,
    provider: &str,
    prices: &'a PriceTable,
) -> Option<&'a crate::domain::PriceEntry> {
    prices
        .prices
        .iter()
        .find(|p| {
            model_matches(&p.model, model)
                && p.provider
                    .as_deref()
                    .map(|prov| provider_matches(prov, provider))
                    .unwrap_or(false)
        })
        .or_else(|| {
            prices
                .prices
                .iter()
                .find(|p| model_matches(&p.model, model) && p.provider.is_none())
        })
}

/// 精确匹配优先；大小写不一致（如来源上报 `"GPT-4o"`、用户价目表填 `"gpt-4o"`）时仍按同一模型兜底。
fn model_matches(entry_model: &str, record_model: &str) -> bool {
    entry_model == record_model || entry_model.eq_ignore_ascii_case(record_model)
}

fn provider_matches(entry_provider: &str, record_provider: &str) -> bool {
    entry_provider == record_provider || entry_provider.eq_ignore_ascii_case(record_provider)
}

pub fn sum_costs(records: &[&UsageRecord], prices: &PriceTable) -> (Option<f64>, bool) {
    accumulate_costs(records.iter().map(|record| derive_cost(record, prices)))
}

/// Cursor 账号事件没有 native_cost，按模型走用户价目 / LiteLLM 快照。
pub fn sum_cursor_event_costs(
    events: &[&CursorUsageEvent],
    prices: &PriceTable,
) -> (Option<f64>, bool) {
    accumulate_costs(events.iter().map(|event| {
        derive_priced(
            PricedTokens {
                model: &event.model,
                provider: "",
                input_tokens: event.input_tokens,
                output_tokens: event.output_tokens,
                cache_read_tokens: event.cache_read_tokens,
                cache_creation_tokens: event.cache_creation_tokens,
                native_cost: None,
            },
            prices,
        )
    }))
}

fn accumulate_costs(derived: impl IntoIterator<Item = DerivedCost>) -> (Option<f64>, bool) {
    let mut total = 0.0;
    let mut any = false;
    let mut unpriced = false;
    for item in derived {
        if let Some(amount) = item.amount {
            total += amount;
            any = true;
        }
        if item.unpriced {
            unpriced = true;
        }
    }
    (if any { Some(total) } else { None }, unpriced)
}
