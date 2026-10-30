use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::supplier_products::repository;

/// Cascade entry point called from `suppliers::service::delete_supplier`.
pub(crate) async fn delete_links_for_supplier(db: &Db, supplier_key: &str) -> AppResult<u64> {
    repository::delete_links_by_supplier(db, supplier_key).await
}
