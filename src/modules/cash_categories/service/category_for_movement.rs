use super::require_category::require_category;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::sessions::CashMovementType;
use crate::modules::cash_categories::model::CashCategoryDocument;

/// The category a new movement is filed under: it must exist, belong to the movement's type
/// and still be offered. Returns the document so the caller can read its approval rules.
pub(crate) async fn category_for_movement(
    db: &Db,
    key: &str,
    movement_type: CashMovementType,
) -> AppResult<CashCategoryDocument> {
    let doc = require_category(db, key).await?;
    if doc.movement_type != movement_type {
        return Err(AppError::validation_with_code(
            "This category belongs to a different kind of payment.",
            codes::CASH_CATEGORY_WRONG_TYPE,
        ));
    }
    if !doc.active || doc.merged_into_key.is_some() {
        return Err(AppError::conflict(
            codes::CASH_CATEGORY_HIDDEN,
            "This category is hidden. Please choose another one.",
        ));
    }
    Ok(doc)
}
