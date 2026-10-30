use crate::core::error::AppResult;

use crate::domain::billing::CreditNote;

use crate::modules::billing::model::CreditNoteDocument;

use mongodb::bson::Document;

/// Converts a page of raw `credit_notes` documents — as read by the sync
/// module's cursor scan — into the `CreditNote` shape the REST reads return.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<CreditNote>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<CreditNoteDocument>(document)?.into_credit_note())
        })
        .collect()
}
