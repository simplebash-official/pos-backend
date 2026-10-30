use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::core::utils::shop_today_range;
use crate::domain::billing::BillingStats;

use crate::modules::billing::repository;

/// Computes the 4 dashboard KPI cards for the Sales & Invoices History
/// screen. See `shop_today_range` for how "today" is bounded.
pub async fn get_billing_stats(db: &Db) -> AppResult<BillingStats> {
    let (today_start, today_end) = shop_today_range();
    let agg = repository::aggregate_stats(db, today_start, today_end).await?;

    let avg_basket_cents = if agg.today_invoice_count > 0 {
        (agg.today_sales_cents as f64 / agg.today_invoice_count as f64).round() as i64
    } else {
        0
    };

    Ok(BillingStats {
        today_sales_cents: agg.today_sales_cents,
        today_invoice_count: agg.today_invoice_count,
        outstanding_credit_cents: agg.outstanding_credit_cents,
        avg_basket_cents,
    })
}
