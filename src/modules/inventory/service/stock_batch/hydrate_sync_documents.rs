use crate::core::error::AppResult;

use crate::domain::inventory::StockBatch;
use crate::modules::inventory::model::StockBatchDocument;

/// Converts a page of raw `stock_batches` documents — as read by the sync
/// module's cursor scan — into the `StockBatch` shape the REST reads return.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<StockBatch>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<StockBatchDocument>(document)?.into_stock_batch())
        })
        .collect()
}
