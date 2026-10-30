use super::require_category::require_category;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::cash_categories::ReportGroup;
use crate::modules::cash_categories::model::CashCategoryDocument;

/// A category an income can be filed under: visible, and counted as income.
pub(crate) async fn category_for_income(db: &Db, key: &str) -> AppResult<CashCategoryDocument> {
    let doc = require_category(db, key).await?;
    if !doc.active || doc.merged_into_key.is_some() {
        return Err(AppError::conflict(
            codes::CASH_CATEGORY_HIDDEN,
            "This category is hidden. Please choose another one.",
        ));
    }
    if doc.report_group != ReportGroup::OtherIncome {
        return Err(AppError::validation_with_code(
            "This category is not income. Please choose another one.",
            codes::CASH_CATEGORY_WRONG_TYPE,
        ));
    }
    Ok(doc)
}
