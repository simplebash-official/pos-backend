use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PATCH /batches/{key}`. Either field may be sent alone. Changing
/// the cost is Admin-only (the handler checks it).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStockBatchRequest {
    /// New selling price for the batch's remaining units, in cents.
    pub selling_price_cents: Option<i64>,
    /// Corrected unit cost, in cents (Admin only).
    pub unit_cost_cents: Option<i64>,
}
