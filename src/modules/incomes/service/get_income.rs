use super::ledger_row::ledger_row;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::incomes::IncomeRow;
use crate::modules::cash_categories;
use crate::modules::incomes::repository;

/// One ledger income.
pub async fn get_income(db: &Db, key: &str) -> AppResult<IncomeRow> {
    let income = repository::find_income_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Income not found.", codes::INCOME_NOT_FOUND)
        })?;
    let categories = cash_categories::service::resolve_categories(
        db,
        std::slice::from_ref(&income.category_key),
    )
    .await?;
    Ok(ledger_row(&income, &categories[&income.category_key]))
}
