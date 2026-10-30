use super::calc_settlement_part_reply::CalcSettlementPartReply;
use super::settlement_blocker::SettlementBlocker;

use serde::Serialize;

use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSettlementReply {
    /// What every open bill still owes together.
    pub owed_cents: i64,
    /// What the person's split puts against bills.
    pub allocated_cents: i64,
    /// The payment spread over the open bills, oldest first, each taking at most what it owes. What
    /// is left after every bill is paid stays unallocated (it still lowers what the supplier is owed).
    pub oldest_first: Vec<CalcSettlementPartReply>,
    /// Every open bill paid in full: what "Pay everything owed" puts against bills.
    pub pay_everything: Vec<CalcSettlementPartReply>,
    /// What ticking each open bill would put against it now: what is left of the payment, or the
    /// whole bill when nothing is left, never more than the bill owes.
    pub if_ticked: Vec<CalcSettlementPartReply>,
    /// What is wrong with the person's split, in the order to say it; empty means it is fine.
    pub blockers: Vec<SettlementBlocker>,
}
