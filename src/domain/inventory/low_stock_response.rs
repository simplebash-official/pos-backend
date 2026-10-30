use super::low_stock_item::LowStockItem;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /products/low-stock`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LowStockResponse {
    /// List of products requiring restocking.
    pub items: Vec<LowStockItem>,
    /// Total count of low stock items.
    pub total: u64,
}
