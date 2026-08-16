use crate::domain::{DerivedCost, PriceTable, UsageRecord};

pub fn derive_cost(record: &UsageRecord, prices: &PriceTable) -> DerivedCost {
    if let Some(amount) = record.native_cost {
        return DerivedCost {
            amount: Some(amount),
            unpriced: false,
            source_native: true,
        };
    }
    if let Some(entry) = find_price(record, prices) {
        let amount = (record.input_tokens as f64) * entry.input
            + (record.output_tokens as f64) * entry.output
            + (record.cache_read_tokens as f64) * entry.cache_read
            + (record.cache_creation_tokens as f64) * entry.cache_creation;
        return DerivedCost {
            amount: Some(amount),
            unpriced: false,
            source_native: false,
        };
    }
    DerivedCost {
        amount: None,
        unpriced: true,
        source_native: false,
    }
}

fn find_price<'a>(
    record: &UsageRecord,
    prices: &'a PriceTable,
) -> Option<&'a crate::domain::PriceEntry> {
    prices
        .prices
        .iter()
        .find(|p| {
            p.model == record.model
                && p.provider
                    .as_ref()
                    .map(|prov| prov == &record.provider)
                    .unwrap_or(false)
        })
        .or_else(|| {
            prices
                .prices
                .iter()
                .find(|p| p.model == record.model && p.provider.is_none())
        })
}

pub fn sum_costs(records: &[UsageRecord], prices: &PriceTable) -> (Option<f64>, bool) {
    let mut total = 0.0;
    let mut any = false;
    let mut unpriced = false;
    for record in records {
        let derived = derive_cost(record, prices);
        if let Some(amount) = derived.amount {
            total += amount;
            any = true;
        }
        if derived.unpriced {
            unpriced = true;
        }
    }
    (if any { Some(total) } else { None }, unpriced)
}
