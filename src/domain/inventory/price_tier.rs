use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One sellable price of a product: every in-stock batch sharing this
/// `selling_price_cents`, merged. The cashier picks a tier, and the server
/// consumes that tier's batches oldest-first (see
/// `inventory::service::stock_batch::plan_tier_allocation`), so two
/// deliveries sold at the same price read as one option while each keeps its
/// own cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PriceTier {
    /// Selling price of every unit in this tier, in cents.
    pub selling_price_cents: i64,
    /// Units left across the tier's batches.
    pub quantity: i64,
    /// Cheapest batch cost in the tier. Admin only (see `Product::without_cost`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_unit_cost_cents: Option<i64>,
    /// Dearest batch cost in the tier. Admin only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_unit_cost_cents: Option<i64>,
    /// When the oldest batch in the tier was received — the one a sale from
    /// this tier draws on first.
    pub oldest_received_at: DateTime<Utc>,
}
