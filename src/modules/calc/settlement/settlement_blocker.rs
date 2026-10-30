use super::settlement_blocker_code::SettlementBlockerCode;

use serde::Serialize;

use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettlementBlocker {
    pub code: SettlementBlockerCode,
    /// The bill, for the bill blockers.
    pub receipt_key: Option<String>,
    /// What the bill still owes, for `over_bill`.
    pub due_cents: Option<i64>,
}
