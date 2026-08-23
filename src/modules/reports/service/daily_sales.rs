// Business logic for the Daily Sales reporting endpoint.

use mongodb::Database;

use super::dates;
use crate::{
    core::{error::AppResult, utils::today_utc_range},
    domain::reports::{DailySalesQuery, DailySalesReportResponse, PaymentMethodBreakdown},
    modules::reports::repository,
};

/// Generates daily sales summaries with breakdowns by stream and payment methods.
pub(crate) async fn get_daily_sales_report(
    db: &Database,
    query: DailySalesQuery,
) -> AppResult<DailySalesReportResponse> {
    let (start, end) = if let Some(d) = query.date.filter(|s| !s.trim().is_empty()) {
        let start = dates::parse_iso_or_ymd(d.trim(), false)?;
        let end = dates::parse_iso_or_ymd(d.trim(), true)?;
        (start, end)
    } else if query.from.is_some() || query.to.is_some() {
        dates::parse_date_range(None, query.from.as_deref(), query.to.as_deref())?
    } else {
        today_utc_range()
    };

    let summaries = repository::sales::aggregate_daily_sales(db, start, end).await?;

    let mut total_sales_cents = 0;
    let mut total_invoices = 0;
    let mut total_repair_revenue_cents = 0;
    let mut total_print_revenue_cents = 0;
    let mut total_retail_revenue_cents = 0;
    let mut total_discounts_cents = 0;
    let mut payment_methods = PaymentMethodBreakdown::default();

    for s in &summaries {
        total_sales_cents += s.total_sales_cents;
        total_invoices += s.total_invoices;
        total_repair_revenue_cents += s.repair_revenue_cents;
        total_print_revenue_cents += s.print_revenue_cents;
        total_retail_revenue_cents += s.retail_revenue_cents;
        total_discounts_cents += s.discount_cents;

        payment_methods.cash_cents += s.payment_methods.cash_cents;
        payment_methods.card_cents += s.payment_methods.card_cents;
        payment_methods.online_cents += s.payment_methods.online_cents;
        payment_methods.credit_cents += s.payment_methods.credit_cents;
    }

    Ok(DailySalesReportResponse {
        summaries,
        total_sales_cents,
        total_invoices,
        total_repair_revenue_cents,
        total_print_revenue_cents,
        total_retail_revenue_cents,
        total_discounts_cents,
        payment_methods,
    })
}
