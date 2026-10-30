use super::month_string::month_string;
use super::parse_month::parse_month;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::cash_categories::recurring_expense_key;
use crate::domain::recurring_expenses::{PeriodStatus, PeriodStatusQuery};
use crate::modules::expenses;
use crate::modules::recurring_expenses::repository;

/// What is already recorded for a category and payee in a month, for the "already paid" warning.
pub async fn period_status(db: &Db, query: PeriodStatusQuery) -> AppResult<PeriodStatus> {
    let month = parse_month(&query.period)?;
    let period = month_string(month);
    let payments = expenses::service::period_payments(
        db,
        &query.category_key,
        query.payee_key.as_deref(),
        &period,
    )
    .await?;
    let paid_cents: i64 = payments.iter().map(|p| p.amount_cents).sum();
    let template_key = recurring_expense_key(&query.category_key, query.payee_key.as_deref());
    let expected_cents = repository::find_by_key(db, &template_key)
        .await?
        .filter(|t| t.active)
        .map(|t| t.amount_cents);
    Ok(PeriodStatus {
        period,
        paid_cents,
        expected_cents,
        already_paid: match expected_cents {
            Some(expected) => paid_cents >= expected,
            None => paid_cents > 0,
        },
        payments,
    })
}
