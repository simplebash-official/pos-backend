use super::detail_of::detail_of;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::expenses::ExpenseDetail;
use crate::modules::expenses::repository;

/// One ledger expense with its payments.
pub async fn get_expense(db: &Db, key: &str) -> AppResult<ExpenseDetail> {
    let expense = repository::find_expense_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Expense not found.", codes::EXPENSE_NOT_FOUND)
        })?;
    detail_of(db, &expense).await
}
