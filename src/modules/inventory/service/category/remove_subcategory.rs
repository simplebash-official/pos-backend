use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::CategoryInfo;
use crate::modules::inventory::model::SubcategoryDocument;
use crate::modules::inventory::repository;
use axum::http::StatusCode;

/// Removes a subcategory. Checks existence within this category (404
/// `SUBCATEGORY_NOT_FOUND`) and, like `delete_category`, refuses (409
/// `SUBCATEGORY_IN_USE`) while any product still references this exact
/// `subcategory_key`.
pub(crate) async fn remove_subcategory(
    db: &Db,
    category_key: String,
    subcategory_key: String,
) -> AppResult<CategoryInfo> {
    crate::core::logging::domain::tracked("inventory.subcategory_removed", async move {
        let category = repository::category::find_category_by_key(db, &category_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
            })?;

        let subcategory = repository::subcategory::find_subcategory_by_key(db, &subcategory_key)
            .await?
            .filter(|document| document.category_key == category_key)
            .ok_or_else(|| {
                AppError::not_found_with_code("Subcategory not found", codes::SUBCATEGORY_NOT_FOUND)
            })?;

        let products_using_subcategory =
            repository::product::count_products_in_subcategory(db, &subcategory_key).await?;

        if products_using_subcategory > 0 {
            return Err(AppError::custom(
                StatusCode::CONFLICT,
                codes::SUBCATEGORY_IN_USE,
                format!(
                    "{products_using_subcategory} product(s) still reference subcategory '{}'",
                    subcategory.name
                ),
            ));
        }

        repository::subcategory::delete_subcategory_by_key(db, &subcategory_key).await?;

        let subcategory_documents =
            repository::subcategory::list_subcategories_by_category(db, &category_key).await?;
        let subcategories = subcategory_documents
            .into_iter()
            .map(SubcategoryDocument::into_subcategory_info)
            .collect();

        Ok(category.into_category_info(subcategories))
    })
    .await
}
