use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// Response for a stock adjustment — includes both the before/after
/// quantities so a client can display the change without a second request.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockAdjustmentResponse {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Product business key.
    pub key: String,
    /// Product SKU code.
    pub sku: String,
    /// Product name.
    pub name: String,
    /// New stock level after adjustment.
    pub stock_quantity: i64,
    /// Stock level prior to adjustment.
    pub previous_stock_quantity: i64,
    /// Quantity difference applied (+/-).
    pub delta: i64,
    /// Timestamp of adjustment.
    pub updated_at: DateTime<Utc>,
}
