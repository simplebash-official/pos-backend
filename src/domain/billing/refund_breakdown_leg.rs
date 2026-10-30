use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One method/amount leg of a credit note's cash refund payout — plural
/// because a credit note against a split-paid invoice must refund across
/// the same methods (for till/card-settlement reconciliation), not as one
/// lump sum.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundBreakdownLeg {
    /// Payment method this leg refunds ("cash", "card", "online").
    pub method: String,
    /// Amount refunded via this method in cents.
    pub amount_cents: i64,
}
