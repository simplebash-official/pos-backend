use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::CategoriesResponse;
use crate::modules::inventory::model::SubcategoryDocument;
use crate::modules::inventory::repository;

/// All categories with their subcategories, alphabetically sorted — a single
/// `$lookup` aggregation round trip (see
/// `repository::category::list_categories_with_subcategories`) rather than a
/// separate subcategory query grouped by `category_key` in memory.
pub async fn list_categories(db: &Db) -> AppResult<CategoriesResponse> {
    let categories = repository::category::list_categories_with_subcategories(db)
        .await?
        .into_iter()
        .map(|(category, subcategories)| {
            let subcategories = subcategories
                .into_iter()
                .map(SubcategoryDocument::into_subcategory_info)
                .collect();
            category.into_category_info(subcategories)
        })
        .collect();

    Ok(CategoriesResponse { categories })
}
