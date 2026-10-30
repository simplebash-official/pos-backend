use super::purchase::Purchase;
use crate::domain::inventory::PaginationMeta;

use serde::Serialize;
use utoipa::ToSchema;

/// Paginated response for `GET /purchases`. `items` and `purchases` carry the
/// same rows under both names — `purchases` is the shape the frontend already
/// reads, `items` the generic one the sync client expects.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseListResponse {
    /// List of purchase history records.
    pub items: Vec<Purchase>,
    /// Duplicate list alias for backwards-compatibility.
    pub purchases: Vec<Purchase>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}
