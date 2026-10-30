use crate::core::error::AppResult;

use crate::domain::quotations::SyncQuotation;

use crate::modules::quotations::model::QuotationDocument;

use mongodb::bson;
use mongodb::bson::Document;

/// Converts a page of raw `quotations` documents — as read by the sync module's
/// cursor scan — into the wire shape devices and the cloud exchange. Every sync
/// feed (pull, snapshot, push, outbox) goes through `sync::service::hydrate`,
/// and a resource without an arm there silently syncs nothing.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<SyncQuotation>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<QuotationDocument>(document)?
                    .into_sync_quotation(),
            )
        })
        .collect()
}
