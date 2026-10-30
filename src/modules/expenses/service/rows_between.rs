use super::all_rows::all_rows;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::expenses::ExpenseRow;

/// Every cost dated between two days (`YYYY-MM-DD`, inclusive), drawer payments included, for the money reports.
pub(crate) async fn rows_between(db: &Db, from: &str, to: &str) -> AppResult<Vec<ExpenseRow>> {
    all_rows(db, Some(from), Some(to)).await
}
