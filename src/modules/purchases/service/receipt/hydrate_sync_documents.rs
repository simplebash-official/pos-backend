use crate::core::error::{AppError, AppResult};

use crate::domain::purchase_receipts::PurchaseReceipt;

use crate::modules::purchases::model::{PurchaseReceiptDocument, ReceiptFigures};

/// Converts raw `purchase_receipts` documents, as the sync module reads them, into the wire
/// shape. Only the stored fields matter to the receiver: the supplier, the lines and the total
/// are computed on read, and the receiving side drops them.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<PurchaseReceipt>> {
    documents
        .into_iter()
        .map(|document| {
            let receipt = bson::deserialize_from_document::<PurchaseReceiptDocument>(document)
                .map_err(AppError::from)?;
            Ok(receipt.into_receipt(
                None,
                Vec::new(),
                ReceiptFigures {
                    credited_cents: 0,
                    settlements: Vec::new(),
                    charges: Vec::new(),
                },
            ))
        })
        .collect()
}
