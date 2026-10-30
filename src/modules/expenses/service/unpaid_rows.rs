use super::all_rows::all_rows;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::expenses::ExpenseRow;

/// Every cost with something still to pay, whenever it was dated, for the payables report.
pub(crate) async fn unpaid_rows(db: &Db) -> AppResult<Vec<ExpenseRow>> {
    Ok(all_rows(db, None, None)
        .await?
        .into_iter()
        .filter(|r| r.balance_cents > 0)
        .collect())
}
