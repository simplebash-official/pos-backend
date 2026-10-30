use serde::Deserialize;

use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSettlementPart {
    pub receipt_key: String,
    pub amount_cents: i64,
}
