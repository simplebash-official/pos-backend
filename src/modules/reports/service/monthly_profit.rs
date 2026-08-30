// Business logic for Monthly Profit & Financial Intelligence report.

use chrono::{Datelike, NaiveDate, Utc};
use mongodb::Database;

use crate::{
    core::error::{AppError, AppResult},
    domain::reports::{MonthlyProfitDayEntry, MonthlyProfitQuery, MonthlyProfitReportResponse},
    modules::reports::repository,
};

/// Generates monthly profit and financial intelligence metrics including gross/net profit,
/// commissions, and refund impact.
pub(crate) async fn get_monthly_profit_report(
    db: &Database,
    query: MonthlyProfitQuery,
) -> AppResult<MonthlyProfitReportResponse> {
    let now = Utc::now();
    let current_year = now.year();
    let current_month = now.month();

    let (year, month, year_month_str) =
        if let Some(m_str) = query.month.filter(|s| !s.trim().is_empty()) {
            let parts: Vec<&str> = m_str.trim().split('-').collect();
            if parts.len() != 2 {
                return Err(AppError::validation(
                    "Invalid month format. Expected YYYY-MM (e.g. 2026-08).",
                ));
            }
            let y: i32 = parts[0]
                .parse()
                .map_err(|_| AppError::validation("Invalid year in month string"))?;
            let m: u32 = parts[1]
                .parse()
                .map_err(|_| AppError::validation("Invalid month in month string"))?;
            if !(1..=12).contains(&m) {
                return Err(AppError::validation("Month must be between 01 and 12"));
            }
            (y, m, format!("{y:04}-{m:02}"))
        } else {
            let y = query.year.unwrap_or(current_year);
            let m = current_month;
            (y, m, format!("{y:04}-{m:02}"))
        };

    let start = NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| AppError::validation("Invalid start of month"))?
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is valid")
        .and_utc();

    let next_month_date = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).expect("valid Jan 1")
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).expect("valid next month")
    };
    let end = next_month_date
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is valid")
        .and_utc();

    let sales = repository::sales::aggregate_sales_summary(db, start, end).await?;
    let daily_summaries = repository::sales::aggregate_daily_sales(db, start, end).await?;
    // Only the aggregate totals are needed here — no employee display
    // name/role resolution, so the raw unresolved bucket map from
    // `repository::commissions` is used as-is.
    let commissions =
        repository::commissions::aggregate_employee_commissions(db, start, end, None).await?;
    let total_refunds_cents = repository::refunds::aggregate_refund_totals(db, start, end)
        .await?
        .net_refund_cents;

    let mut cogs_cents = 0;
    let mut commission_payouts_cents = 0;
    for emp in commissions.values() {
        cogs_cents += emp.estimated_cost_cents;
        commission_payouts_cents += emp.earned_commission_cents;
    }

    let gross_profit_cents = (sales.total_sales_cents - cogs_cents).max(0);
    let net_profit_cents =
        (gross_profit_cents - commission_payouts_cents - total_refunds_cents).max(0);

    let mut daily_breakdown = Vec::new();
    for day in daily_summaries {
        let day_revenue = day.total_sales_cents;
        let day_gross_profit = day_revenue;
        let day_commissions = 0;
        let day_net_profit = (day_gross_profit - day_commissions).max(0);

        daily_breakdown.push(MonthlyProfitDayEntry {
            date: day.date,
            revenue_cents: day_revenue,
            cogs_cents: 0,
            gross_profit_cents: day_gross_profit,
            commissions_cents: day_commissions,
            net_profit_cents: day_net_profit,
            invoice_count: day.total_invoices,
        });
    }

    Ok(MonthlyProfitReportResponse {
        year_month: year_month_str,
        total_revenue_cents: sales.total_sales_cents,
        retail_revenue_cents: sales.retail_revenue_cents,
        repair_revenue_cents: sales.repair_revenue_cents,
        print_revenue_cents: sales.print_revenue_cents,
        total_discounts_cents: sales.discount_cents,
        cogs_cents,
        gross_profit_cents,
        commission_payouts_cents,
        net_profit_cents,
        total_refunds_cents,
        total_invoices_count: sales.total_invoices,
        daily_breakdown,
    })
}
