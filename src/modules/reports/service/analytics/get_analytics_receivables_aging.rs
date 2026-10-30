use super::super::dates;
use super::super::dates::RangeBound;

use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::reports::{
    AgingBucket, AgingDebtor, ReceivablesAgingQuery, ReceivablesAgingResponse,
};
use crate::modules::reports::repository;
use chrono::Utc;

// ---------------------------------------------------------------------------
// receivables aging
// ---------------------------------------------------------------------------
pub(crate) async fn get_analytics_receivables_aging(
    db: &Db,
    query: ReceivablesAgingQuery,
) -> AppResult<ReceivablesAgingResponse> {
    let as_of = match query.as_of.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(s) => dates::parse_iso_or_ymd(s.trim(), RangeBound::End)?,
        None => Utc::now(),
    };

    let invoices = repository::analytics::aggregate_receivables_aging(db, as_of).await?;

    // [min, max] day boundaries; max None == open-ended.
    let defs: [(&str, i32, Option<i32>); 5] = [
        ("Current", i32::MIN, Some(0)),
        ("1–30 days", 1, Some(30)),
        ("31–60 days", 31, Some(60)),
        ("61–90 days", 61, Some(90)),
        ("90+ days", 91, None),
    ];
    let mut buckets: Vec<AgingBucket> = defs
        .iter()
        .map(|(label, min, max)| AgingBucket {
            label: label.to_string(),
            min_days: if *min == i32::MIN { 0 } else { *min },
            max_days: *max,
            invoice_count: 0,
            amount_cents: 0,
        })
        .collect();

    let mut total_outstanding_cents = 0;
    let mut debtors: std::collections::HashMap<Option<String>, AgingDebtor> =
        std::collections::HashMap::new();

    for inv in &invoices {
        total_outstanding_cents += inv.balance_due_cents;
        let bi = defs
            .iter()
            .position(|(_, min, max)| {
                inv.days_overdue >= *min && max.map(|m| inv.days_overdue <= m).unwrap_or(true)
            })
            .unwrap_or(0);
        buckets[bi].invoice_count += 1;
        buckets[bi].amount_cents += inv.balance_due_cents;

        if inv.days_overdue >= 61 {
            let d = debtors
                .entry(inv.customer_key.clone())
                .or_insert_with(|| AgingDebtor {
                    customer_key: inv.customer_key.clone(),
                    customer_name: inv.customer_name.clone(),
                    customer_phone: inv.customer_phone.clone(),
                    outstanding_cents: 0,
                    oldest_days: 0,
                    invoice_count: 0,
                });
            d.outstanding_cents += inv.balance_due_cents;
            d.oldest_days = d.oldest_days.max(inv.days_overdue);
            d.invoice_count += 1;
        }
    }

    let mut top_debtors: Vec<AgingDebtor> = debtors.into_values().collect();
    top_debtors.sort_by_key(|d| std::cmp::Reverse(d.outstanding_cents));
    top_debtors.truncate(10);

    Ok(ReceivablesAgingResponse {
        as_of,
        buckets,
        total_outstanding_cents,
        total_invoices: invoices.len() as u64,
        top_debtors,
    })
}
