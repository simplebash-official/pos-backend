use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PUT /supplier-products/bulk/{supplierKey}` — the full set of
/// product keys a supplier should be linked to after this call. Any
/// existing link not in this list is removed; any new one is added; ones
/// that survive keep their existing `costPriceCents`/`notes` (see
/// `service::link::replace_links_for_supplier`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BulkReplaceLinksRequest {
    /// List of product keys that should be linked to the supplier.
    pub product_keys: Vec<String>,
}
