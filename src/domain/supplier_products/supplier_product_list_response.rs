use super::supplier_product_link::SupplierProductLink;
use crate::domain::inventory::PaginationMeta;

use serde::Serialize;
use utoipa::ToSchema;

/// Paginated response for `GET /supplier-products`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierProductListResponse {
    /// List of link records.
    pub items: Vec<SupplierProductLink>,
    /// Duplicate list alias for backwards-compatibility.
    pub links: Vec<SupplierProductLink>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}
