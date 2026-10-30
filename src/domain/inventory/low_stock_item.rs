use serde::Serialize;
use utoipa::ToSchema;

/// One entry in the low-stock report. `deficit` (threshold minus current
/// quantity) is precomputed here so clients don't each reimplement the
/// subtraction.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LowStockItem {
    /// Product MongoDB hex ID.
    pub id: String,
    /// Product business key.
    pub key: String,
    /// Product SKU code.
    pub sku: String,
    /// Product name.
    pub name: String,
    /// Current stock on hand.
    pub stock_quantity: i64,
    /// Threshold configured for alerts.
    pub min_stock_threshold: i64,
    /// Units below threshold (min_stock_threshold - stock_quantity).
    pub deficit: i64,
}
