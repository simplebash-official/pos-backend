use super::all_rows::all_rows;
use super::today::today;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::expenses::{ExpenseSource, ExpenseStats};

/// Headline figures: this month's costs, and what is still owed.
pub async fn get_stats(db: &Db) -> AppResult<ExpenseStats> {
    let rows = all_rows(db, None, None).await?;
    let month = &today()[..7];
    let mut stats = ExpenseStats {
        this_month_cents: 0,
        this_month_drawer_cents: 0,
        unpaid_cents: 0,
        unpaid_count: 0,
        overdue_cents: 0,
        overdue_count: 0,
    };
    for r in &rows {
        if r.date.starts_with(month) {
            stats.this_month_cents += r.amount_cents;
            if r.source == ExpenseSource::Drawer {
                stats.this_month_drawer_cents += r.amount_cents;
            }
        }
        if r.balance_cents > 0 {
            stats.unpaid_cents += r.balance_cents;
            stats.unpaid_count += 1;
            if r.is_overdue {
                stats.overdue_cents += r.balance_cents;
                stats.overdue_count += 1;
            }
        }
    }
    Ok(stats)
}
