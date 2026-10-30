use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// A product at or below its low-stock line.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseLowStock {
    pub product_id: String,
    pub reference: String,
    pub name: String,
    pub stock_quantity: i64,
    /// The line it is measured against.
    pub threshold: i64,
    pub out_of_stock: bool,
    pub updated_at: DateTime<Utc>,
    /// Whole days since the product last changed.
    pub days_since_update: i64,
}
