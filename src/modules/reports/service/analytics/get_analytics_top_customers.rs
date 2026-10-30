use super::super::dates;

use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::reports::{TopCustomerEntry, TopCustomersQuery, TopCustomersResponse};
use crate::modules::reports::repository;

// ---------------------------------------------------------------------------
// top customers
// ---------------------------------------------------------------------------
pub(crate) async fn get_analytics_top_customers(
    db: &Db,
    query: TopCustomersQuery,
) -> AppResult<TopCustomersResponse> {
    let (start, end) = dates::parse_date_range_tz(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;
    let limit = query.limit.unwrap_or(100).clamp(1, 100) as usize;
    let sort_by = query.sort_by.as_deref().unwrap_or("revenue");

    let mut rows = repository::analytics::aggregate_top_customers(db, start, end).await?;
    let total_customers = rows.len() as u64;
    let total_revenue_cents: i64 = rows.iter().map(|r| r.revenue_cents).sum();

    rows.sort_by(|a, b| match sort_by {
        "invoices" => b.invoice_count.cmp(&a.invoice_count),
        "profit" => b.gross_profit_cents.cmp(&a.gross_profit_cents),
        _ => b.revenue_cents.cmp(&a.revenue_cents),
    });
    rows.truncate(limit);

    let keys: Vec<String> = rows.iter().filter_map(|r| r.customer_key.clone()).collect();
    let balances = repository::analytics::fetch_customer_balances(db, &keys).await?;

    let customers = rows
        .into_iter()
        .map(|r| {
            let outstanding_cents = r
                .customer_key
                .as_ref()
                .and_then(|k| balances.get(k))
                .copied()
                .unwrap_or(0);
            TopCustomerEntry {
                is_walk_in: r.customer_key.is_none(),
                customer_key: r.customer_key,
                customer_name: r.customer_name,
                customer_phone: r.customer_phone,
                invoice_count: r.invoice_count,
                revenue_cents: r.revenue_cents,
                discount_cents: r.discount_cents,
                gross_profit_cents: r.gross_profit_cents,
                outstanding_cents,
                last_purchase_at: r.last_purchase_at,
            }
        })
        .collect();

    Ok(TopCustomersResponse {
        customers,
        total_customers,
        total_revenue_cents,
    })
}
