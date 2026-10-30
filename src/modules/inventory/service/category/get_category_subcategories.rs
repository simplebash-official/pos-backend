use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::SubcategoriesResponse;
use crate::modules::inventory::model::SubcategoryDocument;
use crate::modules::inventory::repository;

/// The subcategories for one category, 404ing if the category itself
/// doesn't exist.
pub(crate) async fn get_category_subcategories(
    db: &Db,
    category_key: String,
) -> AppResult<SubcategoriesResponse> {
    repository::category::find_category_by_key(db, &category_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    let subcategory_documents =
        repository::subcategory::list_subcategories_by_category(db, &category_key).await?;
    let subcategories = subcategory_documents
        .into_iter()
        .map(SubcategoryDocument::into_subcategory_info)
        .collect();

    Ok(SubcategoriesResponse {
        category_key,
        subcategories,
    })
}
