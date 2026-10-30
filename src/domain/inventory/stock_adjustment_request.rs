use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PATCH /products/{id}/stock`. `delta` is signed rather than an
/// absolute target quantity, so concurrent adjustments compose instead of
/// racing to overwrite each other's target value. Stock always lives in a
/// batch, so a negative delta names the batch it is taken from (`batchKey`),
/// and a positive delta brings in a new `adjustment` batch, which needs its
/// own cost and selling price.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockAdjustmentRequest {
    /// Signed integer change in stock quantity (+/-).
    pub delta: i64,
    /// Reason or note for manual stock adjustment.
    #[serde(default)]
    pub reason: Option<String>,
    /// The batch a negative delta is taken from.
    #[serde(default)]
    pub batch_key: Option<String>,
    /// Unit cost of the units a positive delta brings in, in cents.
    #[serde(default)]
    pub unit_cost_cents: Option<i64>,
    /// Selling price of the units a positive delta brings in, in cents.
    #[serde(default)]
    pub selling_price_cents: Option<i64>,
}
