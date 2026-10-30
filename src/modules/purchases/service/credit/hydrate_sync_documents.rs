use crate::core::error::{AppError, AppResult};

use crate::domain::supplier_credits::SupplierCredit;
use crate::modules::purchases::model::SupplierCreditDocument;

/// Converts raw `supplier_credits` documents, as the sync module reads them, into the wire shape.
/// `voidedByKey` is worked out when credits are listed and is dropped by the receiver.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<SupplierCredit>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<SupplierCreditDocument>(document)
                    .map_err(AppError::from)?
                    .into_credit(None),
            )
        })
        .collect()
}
