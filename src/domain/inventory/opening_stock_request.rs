use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Units a product is created with when they are not a supplier purchase.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpeningStockRequest {
    /// Units on hand.
    pub quantity: i64,
    /// Cost of one unit in cents.
    pub unit_cost_cents: i64,
    /// Price one unit sells at, in cents.
    pub selling_price_cents: i64,
}
