use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How many units of one sale/return line came out of (or went back into)
/// one stock batch, and what each of those units cost. A line at one price
/// tier can span several batches (oldest first), so an invoice line carries
/// a list of these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchAllocation {
    /// The batch the units came from.
    pub batch_key: String,
    /// Units taken from that batch.
    pub quantity: i64,
    /// Cost of one of those units, in cents (the batch's cost at sale time).
    pub unit_cost_cents: i64,
}
