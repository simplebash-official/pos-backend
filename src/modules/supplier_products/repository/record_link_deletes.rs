use crate::clients::db::Db;

use crate::core::error::AppResult;

/// Tells cloud sync these links were removed outright (see
/// `sync::service::record_hard_deletes`).
pub(super) async fn record_link_deletes(db: &Db, keys: Vec<String>) -> AppResult<()> {
    crate::modules::sync::service::record_hard_deletes(db, "supplier_products", &keys).await
}
