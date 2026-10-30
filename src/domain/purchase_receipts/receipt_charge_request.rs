use super::allocation::Allocation;
use super::billed_by::BilledBy;
use super::charge_kind::ChargeKind;

use serde::Deserialize;
use utoipa::ToSchema;

/// One extra charge on a receipt being recorded.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptChargeRequest {
    pub kind: ChargeKind,
    /// Always positive; a discount is a `discount` kind, not a negative amount.
    pub amount_cents: i64,
    pub billed_by: BilledBy,
    /// How it is spread over the products. Required when the receipt has two or more lines,
    /// refused when it has one.
    #[serde(default)]
    pub allocation: Option<Allocation>,
    /// The saved payee (courier, agent) a third-party charge is paid to. Required for
    /// `third_party`, refused for `supplier`.
    #[serde(default)]
    pub payee_key: Option<String>,
    /// What an `other` charge is, in a few words (at most 80 characters). Optional, and refused
    /// for every other kind.
    #[serde(default)]
    pub description: Option<String>,
}
