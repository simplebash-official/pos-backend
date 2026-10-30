use crate::core::error::{AppError, AppResult};

use crate::domain::purchase_receipts::ReceiptCharge;

use crate::modules::purchases::model::ReceiptChargeDocument;

/// Converts raw `receipt_charges` documents, as the sync module reads them, into the wire shape.
pub(crate) fn hydrate_charge_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<ReceiptCharge>> {
    documents
        .into_iter()
        .map(|document| {
            bson::deserialize_from_document::<ReceiptChargeDocument>(document)
                .map(ReceiptChargeDocument::into_charge)
                .map_err(AppError::from)
        })
        .collect()
}
