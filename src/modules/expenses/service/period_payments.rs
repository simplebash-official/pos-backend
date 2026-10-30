use super::all_rows::all_rows;
use super::counted_month::counted_month;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::recurring_expenses::PeriodPayment;
use crate::modules::cash_categories;

/// What is already recorded for a category (and payee, when given) in one month, from the ledger and
/// the drawer, with reversed drawer payments left out. Used for regular bills and the "already paid" warning.
pub(crate) async fn period_payments(
    db: &Db,
    category_key: &str,
    payee_key: Option<&str>,
    month: &str,
) -> AppResult<Vec<PeriodPayment>> {
    let category_keys = cash_categories::service::keys_including_merged(db, category_key).await?;
    Ok(all_rows(db, None, None)
        .await?
        .into_iter()
        .filter(|r| r.paid_cents > 0)
        .filter(|r| category_keys.contains(&r.category_key))
        .filter(|r| payee_key.is_none_or(|p| r.payee_key.as_deref() == Some(p)))
        .filter(|r| counted_month(r) == month)
        .map(|r| PeriodPayment {
            number: r.number,
            date: r.date,
            amount_cents: r.paid_cents,
            paid_from: r.paid_from,
        })
        .collect())
}
