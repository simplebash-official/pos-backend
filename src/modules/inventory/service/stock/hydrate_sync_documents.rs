use crate::core::error::AppResult;

use crate::modules::inventory::model::StockMovementDocument;

/// Converts a page of raw `stock_movements` documents — as read by the sync
/// module's cursor scan — into the `StockMovement` shape the REST reads
/// return. Needs no lookups: a movement carries everything it exposes.
/// See `service::product::hydrate_sync_documents` for why the two feeds
/// must agree.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<crate::domain::inventory::StockMovement>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<StockMovementDocument>(document)?
                    .into_stock_movement(),
            )
        })
        .collect()
}
