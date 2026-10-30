use serde::Deserialize;
use utoipa::ToSchema;

/// Part of a payment counted against one receipt, when paying a supplier later.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptSettlementRequest {
    pub receipt_key: String,
    /// At most what is still due on that receipt.
    pub amount_cents: i64,
}
