use crate::core::error::AppResult;

use crate::domain::repairs::Repair;

use crate::modules::repairs::model::RepairDocument;

/// Converts a page of raw `repairs` documents — as read by the sync
/// module's cursor scan — into the `Repair` shape the REST reads return.
/// See `suppliers::service::hydrate_sync_documents` for why the delta and
/// snapshot feeds must produce identical rows.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<Repair>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<RepairDocument>(document)?.into_repair())
        })
        .collect()
}
