use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One leg of a split payment. `method` is `"cash"` | `"card"` | `"online"`
/// (never `"split"` — that's the invoice-level `payment_method` value that
/// means "see `split_payments`").
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SplitPayment {
    /// Payment method used for this portion ("cash", "card", "online").
    pub method: String,
    /// Amount paid via this payment method in cents.
    pub amount_cents: i64,
    /// Last 4 digits of card number if paid by card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_last4: Option<String>,
    /// Reference or transaction number for card or online payment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}
