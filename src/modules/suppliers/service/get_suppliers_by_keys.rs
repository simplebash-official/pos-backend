use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::suppliers::Supplier;
use crate::modules::suppliers::model::SupplierDocument;
use crate::modules::suppliers::repository;

/// Cross-module batch lookup — `purchases::service::purchase::list_purchases`
/// calls this to enrich a page of purchase history with supplier display
/// data in one query instead of one `get_supplier_by_key` call per row.
pub(crate) async fn get_suppliers_by_keys(db: &Db, keys: &[String]) -> AppResult<Vec<Supplier>> {
    let documents = repository::find_suppliers_by_keys(db, keys).await?;
    Ok(documents
        .into_iter()
        .map(SupplierDocument::into_supplier)
        .collect())
}
