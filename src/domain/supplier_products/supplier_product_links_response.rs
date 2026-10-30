use super::supplier_product_link::SupplierProductLink;

use serde::Serialize;
use utoipa::ToSchema;

/// Legacy/Array response for `GET /supplier-products`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierProductLinksResponse {
    /// List of supplier-product links.
    pub links: Vec<SupplierProductLink>,
}
