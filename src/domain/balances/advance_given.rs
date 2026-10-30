use serde::Serialize;
use utoipa::ToSchema;

/// One advance handed out.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceGiven {
    /// The voucher number (`CSH-000001`) or the expense number (`EXP-000001`).
    pub number: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub amount_cents: i64,
    pub category_name: String,
}
