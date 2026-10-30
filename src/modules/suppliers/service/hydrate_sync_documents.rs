use crate::core::error::AppResult;

use crate::domain::suppliers::Supplier;
use crate::modules::suppliers::model::SupplierDocument;

use mongodb::bson::Document;

/// Converts a page of raw `suppliers` documents — as read by the sync
/// module's cursor scan — into the `Supplier` shape the REST reads return.
/// See `inventory::service::product::hydrate_sync_documents` for why the
/// delta and snapshot feeds must produce identical rows.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<Supplier>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<SupplierDocument>(document)?.into_supplier())
        })
        .collect()
}
