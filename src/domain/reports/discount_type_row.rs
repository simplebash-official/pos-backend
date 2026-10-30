use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// --- discounts --------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscountTypeRow {
    /// `percentage` | `fixed` | `none`.
    pub discount_type: String,
    pub invoice_count: u64,
    pub discount_cents: i64,
}
