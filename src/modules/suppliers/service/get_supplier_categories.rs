use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::suppliers::SupplierCategoriesResponse;

use crate::modules::suppliers::repository;

/// Every distinct tag currently in use across all suppliers'
/// `suppliedCategories`, sorted for stable output — backs
/// `GET /suppliers/categories`.
pub(crate) async fn get_supplier_categories(db: &Db) -> AppResult<SupplierCategoriesResponse> {
    let mut categories = repository::distinct_supplied_categories(db).await?;
    categories.sort();
    categories.dedup();
    Ok(SupplierCategoriesResponse { categories })
}
