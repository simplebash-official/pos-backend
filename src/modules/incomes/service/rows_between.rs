use super::all_rows::all_rows;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::incomes::IncomeRow;

/// What was received into the bank, a card or online on or after `since` (`YYYY-MM-DD`), as `(account, cents)`.
/// Every income dated between two days (`YYYY-MM-DD`, inclusive), cash added to the drawer included, for the money reports.
pub(crate) async fn rows_between(db: &Db, from: &str, to: &str) -> AppResult<Vec<IncomeRow>> {
    all_rows(db, Some(from), Some(to)).await
}
