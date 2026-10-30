use crate::core::error::AppResult;

use crate::domain::billing::PaymentRecord;
use crate::modules::billing::model::PaymentDocument;

use mongodb::bson::Document;

/// Payments are append-only, same posture as invoices — see
/// `service::hydrate_sync_documents`'s doc comment.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<PaymentRecord>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<PaymentDocument>(document)?.into_payment_record())
        })
        .collect()
}
