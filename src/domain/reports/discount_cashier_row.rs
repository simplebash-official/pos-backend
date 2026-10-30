use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscountCashierRow {
    pub cashier_id: String,
    pub cashier_name: String,
    pub discount_cents: i64,
    pub revenue_cents: i64,
    pub discount_rate_bps: i64,
}
