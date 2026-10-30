use serde::Serialize;

use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSettlementPartReply {
    pub receipt_key: String,
    pub amount_cents: i64,
}
