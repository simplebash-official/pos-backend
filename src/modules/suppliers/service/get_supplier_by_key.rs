use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::suppliers::Supplier;

use crate::modules::suppliers::repository;

/// Same as `get_supplier`, looked up by `key` instead of `ObjectId` — the
/// cross-module entry point `supplier_products`/`purchases` call to
/// validate a `supplierKey` and enrich their own responses.
pub(crate) async fn get_supplier_by_key(db: &Db, key: &str) -> AppResult<Supplier> {
    let document = repository::find_supplier_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Supplier not found", codes::SUPPLIER_NOT_FOUND)
        })?;

    Ok(document.into_supplier())
}
