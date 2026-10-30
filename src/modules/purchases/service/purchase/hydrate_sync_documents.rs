use super::enrich_purchases::enrich_purchases;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};

use crate::domain::purchases::Purchase;

use crate::modules::purchases::model::PurchaseDocument;

/// Converts a page of raw `purchases` documents — as read by the sync
/// module's cursor scan — into the `Purchase` shape `GET /purchases`
/// returns, embedded supplier/product summaries included, so a row that
/// arrives by delta carries the same display data as one from a snapshot.
/// See `inventory::service::product::hydrate_sync_documents`.
pub(crate) async fn hydrate_sync_documents(
    db: &Db,
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<Purchase>> {
    let purchases: Vec<PurchaseDocument> = documents
        .into_iter()
        .map(|document| {
            bson::deserialize_from_document::<PurchaseDocument>(document).map_err(AppError::from)
        })
        .collect::<AppResult<Vec<_>>>()?;
    enrich_purchases(db, purchases).await
}
