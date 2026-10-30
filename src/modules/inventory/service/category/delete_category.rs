use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::CategoryInfo;

use crate::modules::inventory::repository;
use axum::http::StatusCode;

/// Refuses to delete (409 `CATEGORY_IN_USE`) while any product still
/// references this category by `category_key`. Once the guard passes, also
/// cascade-deletes every subcategory under this category — safe because a
/// product can only hold a `subcategory_key` whose owning subcategory has
/// this same `category_key` (enforced by
/// `service::product::ensure_valid_category`), so no product can be left
/// pointing at an orphaned subcategory.
pub(crate) async fn delete_category(db: &Db, category_key: String) -> AppResult<CategoryInfo> {
    crate::core::logging::domain::tracked("inventory.category_deleted", async move {
        let products_using_category =
            repository::product::count_products_in_category(db, &category_key).await?;

        if products_using_category > 0 {
            return Err(AppError::custom(
                StatusCode::CONFLICT,
                codes::CATEGORY_IN_USE,
                format!(
                    "{products_using_category} product(s) still reference category '{category_key}'"
                ),
            ));
        }

        let deleted = repository::category::delete_category(db, &category_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
            })?;

        repository::subcategory::delete_subcategories_by_category(db, &category_key).await?;

        Ok(deleted.into_category_info(Vec::new()))
    })
    .await
}
