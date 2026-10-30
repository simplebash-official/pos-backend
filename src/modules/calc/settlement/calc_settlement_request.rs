use super::calc_settlement_part::CalcSettlementPart;

use serde::Deserialize;

use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSettlementRequest {
    pub supplier_key: String,
    /// What is being paid; 0 until typed.
    pub payment_cents: i64,
    /// How the person split the payment over bills.
    pub settlements: Vec<CalcSettlementPart>,
}
