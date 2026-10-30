use crate::core::error::AppResult;

use crate::domain::customers::Customer;
use crate::modules::customers::model::CustomerDocument;

use mongodb::bson::Document;

/// Converts a page of raw `customers` documents — as read by the sync
/// module's cursor scan — into the `Customer` shape the REST reads return.
/// See `inventory::service::product::hydrate_sync_documents` for why the
/// delta and snapshot feeds must produce identical rows.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<Customer>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<CustomerDocument>(document)?.into_customer())
        })
        .collect()
}
