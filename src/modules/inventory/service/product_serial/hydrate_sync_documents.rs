use crate::core::error::AppResult;

use crate::domain::inventory::ProductSerial;
use crate::modules::inventory::model::ProductSerialDocument;

use mongodb::bson::Document;

/// Converts a page of raw `product_serials` documents — as read by the sync
/// module's cursor scan — into the `ProductSerial` shape the REST read
/// returns. Read-only mirror: the server is the sole writer of this
/// resource, mirroring `stock_movements`'s sync treatment.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<ProductSerial>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<ProductSerialDocument>(document)?
                    .into_product_serial(),
            )
        })
        .collect()
}
