use super::require_category::require_category;
use super::usage_by_key::usage_by_key;
use super::usage_of::usage_of;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::core::logging::domain::tracked;
use crate::domain::cash_categories::{CashCategory, MergeCashCategoryRequest};
use crate::modules::cash_categories::repository;
use mongodb::bson::DateTime as BsonDateTime;

/// Moves a duplicate into another category of the same type. Movements are append-only, so
/// they keep their key; the merged category redirects readers to the target.
pub async fn merge_category(
    db: &Db,
    key: &str,
    body: MergeCashCategoryRequest,
) -> AppResult<CashCategory> {
    tracked("cash_categories.merged", async move {
        if key == body.target_key {
            return Err(AppError::validation(
                "Pick a different category to merge into.",
            ));
        }
        let mut source = require_category(db, key).await?;
        let target = require_category(db, &body.target_key).await?;
        if source.is_system {
            return Err(AppError::validation(
                "A built-in category cannot be merged away. You can hide it instead.",
            ));
        }
        if source.movement_type != target.movement_type {
            return Err(AppError::validation_with_code(
                "Categories can only be merged within the same kind of payment.",
                codes::CASH_CATEGORY_WRONG_TYPE,
            ));
        }
        if !target.active || target.merged_into_key.is_some() {
            return Err(AppError::conflict(
                codes::CASH_CATEGORY_HIDDEN,
                "The category to merge into is hidden.",
            ));
        }
        if source.merged_into_key.is_some() {
            return Err(AppError::conflict(
                codes::CASH_CATEGORY_HIDDEN,
                "This category was already merged.",
            ));
        }

        let now = BsonDateTime::now();
        // Anything already merged into the source now points straight at the target.
        for mut earlier in repository::list_all(db)
            .await?
            .into_iter()
            .filter(|c| c.merged_into_key.as_deref() == Some(source.key.as_str()))
        {
            earlier.merged_into_key = Some(target.key.clone());
            earlier.version += 1;
            earlier.updated_at = now;
            repository::save(db, &earlier).await?;
        }
        source.merged_into_key = Some(target.key.clone());
        source.active = false;
        source.version += 1;
        source.updated_at = now;
        repository::save(db, &source).await?;

        let all = repository::list_all(db).await?;
        let usage = usage_by_key(db).await?;
        let n = usage_of(&target, &all, &usage);
        Ok(target.into_cash_category(n))
    })
    .await
}
