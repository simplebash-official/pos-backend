use crate::core::error::{AppError, AppResult};

use crate::modules::purchases::model::ReceiptSettlementDocument;

/// Converts raw `receipt_settlements` documents, as the sync module reads them, into the wire
/// shape. `reversed` is worked out when a receipt is read and is dropped by the receiver.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<crate::domain::purchase_receipts::ReceiptSettlement>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<ReceiptSettlementDocument>(document)
                    .map_err(AppError::from)?
                    .into_settlement(false),
            )
        })
        .collect()
}
