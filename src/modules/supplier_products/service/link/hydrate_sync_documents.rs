use crate::core::error::AppResult;

use crate::domain::supplier_products::SupplierProductLink;

use crate::modules::supplier_products::model::SupplierProductLinkDocument;

/// Converts a page of raw `supplier_products` documents — as read by the
/// sync module's cursor scan — into the `SupplierProductLink` shape the
/// REST reads return. See
/// `inventory::service::product::hydrate_sync_documents` for why the delta
/// and snapshot feeds must produce identical rows.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<SupplierProductLink>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<SupplierProductLinkDocument>(document)?
                    .into_link(),
            )
        })
        .collect()
}
