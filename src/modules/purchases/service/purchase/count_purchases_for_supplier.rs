use crate::clients::db::Db;
use crate::core::error::AppResult;

use crate::modules::purchases::repository;

/// Used by `suppliers::service::delete_supplier`'s `SUPPLIER_HAS_PURCHASES`
/// guard.
pub(crate) async fn count_purchases_for_supplier(db: &Db, supplier_key: &str) -> AppResult<u64> {
    repository::count_purchases_for_supplier(db, supplier_key).await
}
