use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::supplier_products::repository;

/// Cascade entry point called from
/// `inventory::service::product::delete_product`/`delete_products`.
pub(crate) async fn delete_links_for_product(db: &Db, product_key: &str) -> AppResult<u64> {
    repository::delete_links_by_product(db, product_key).await
}
