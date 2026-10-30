use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcReturnedLineReply {
    pub line_index: usize,
    /// What one unit of this line is worth back.
    pub unit_cents: i64,
    pub refund_cents: i64,
}
