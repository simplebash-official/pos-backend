use serde::Deserialize;
use utoipa::ToSchema;

/// Request body for recording an additional payment against an existing invoice.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordPaymentRequest {
    /// Repayment amount in cents.
    pub amount_cents: i64,
    /// Payment method used ("cash", "card", "online").
    pub payment_method: String,
    /// Optional reference note or description.
    #[serde(default)]
    pub notes: Option<String>,
    /// Register session key if taken during an open cashier shift.
    #[serde(default)]
    pub session_key: Option<String>,
}
