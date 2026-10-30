use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::modules::cash_categories::model::CashCategoryDocument;
use crate::modules::cash_categories::repository;

pub(super) async fn require_category(db: &Db, key: &str) -> AppResult<CashCategoryDocument> {
    repository::find_by_key(db, key).await?.ok_or_else(|| {
        AppError::not_found_with_code("Category not found.", codes::CASH_CATEGORY_NOT_FOUND)
    })
}
