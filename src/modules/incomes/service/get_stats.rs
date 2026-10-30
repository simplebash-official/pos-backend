use super::all_rows::all_rows;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::incomes::{IncomeSource, IncomeStats};
use chrono::{Months, Utc};

/// Headline figures: this month's and last month's other income.
pub async fn get_stats(db: &Db) -> AppResult<IncomeStats> {
    let rows = all_rows(db, None, None).await?;
    let today = Utc::now().date_naive();
    let month = today.format("%Y-%m").to_string();
    let last_month = (today - Months::new(1)).format("%Y-%m").to_string();
    let mut stats = IncomeStats {
        this_month_cents: 0,
        this_month_drawer_cents: 0,
        last_month_cents: 0,
    };
    for r in &rows {
        if r.date.starts_with(&month) {
            stats.this_month_cents += r.amount_cents;
            if r.source == IncomeSource::Drawer {
                stats.this_month_drawer_cents += r.amount_cents;
            }
        } else if r.date.starts_with(&last_month) {
            stats.last_month_cents += r.amount_cents;
        }
    }
    Ok(stats)
}
