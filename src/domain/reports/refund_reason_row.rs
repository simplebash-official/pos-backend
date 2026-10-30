use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// --- refunds ---------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundReasonRow {
    pub reason: String,
    pub credit_note_item_count: u64,
    pub amount_cents: i64,
}
