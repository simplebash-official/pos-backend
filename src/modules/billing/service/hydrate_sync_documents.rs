use crate::core::error::AppResult;

use crate::domain::billing::Invoice;
use crate::modules::billing::model::InvoiceDocument;

use mongodb::bson::Document;

/// Invoices are append-only (no `deleted_at`, no edit/delete route — see
/// `InvoiceDocument`'s doc comment), so unlike every other synced resource
/// this hydrate never needs to worry about tombstones reaching the client.
/// See `repairs::service::hydrate_sync_documents` for why delta and snapshot
/// feeds must produce identical rows.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<Invoice>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<InvoiceDocument>(document)?.into_invoice())
        })
        .collect()
}
