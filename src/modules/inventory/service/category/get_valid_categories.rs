use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::{
    ValidCategoriesResponse, ValidCategoryOption, ValidSubcategoryOption,
};

use crate::modules::inventory::repository;

/// Same underlying data as `list_categories`, reshaped as key+name pairs so
/// a client (e.g. a product-creation form) can display names while
/// submitting `categoryKey`/`subcategoryKey`.
pub(crate) async fn get_valid_categories(db: &Db) -> AppResult<ValidCategoriesResponse> {
    let categories = repository::category::list_categories_with_subcategories(db)
        .await?
        .into_iter()
        .map(|(category, subcategories)| ValidCategoryOption {
            subcategories: subcategories
                .into_iter()
                .map(|document| ValidSubcategoryOption {
                    key: document.key,
                    name: document.name,
                })
                .collect(),
            key: category.key,
            name: category.name,
        })
        .collect();

    Ok(ValidCategoriesResponse { categories })
}
