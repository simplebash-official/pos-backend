use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// --- sales patterns (weekday x hour) -----------------------------------
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PatternCell {
    /// 1 = Monday … 7 = Sunday (`$isoDayOfWeek`).
    pub weekday: u8,
    /// 0–23, shop-local.
    pub hour: u8,
    pub invoice_count: u64,
    pub revenue_cents: i64,
}
