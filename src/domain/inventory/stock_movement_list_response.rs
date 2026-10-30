use super::pagination_meta::PaginationMeta;
use super::stock_movement::StockMovement;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /inventory/stock-movements`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockMovementListResponse {
    /// List of stock movements for current page.
    pub items: Vec<StockMovement>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}
