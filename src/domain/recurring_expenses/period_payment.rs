use crate::domain::expenses::PaidFrom;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One cost already recorded against a bill's month, from the drawer or the ledger.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PeriodPayment {
    /// `EXP-000001` or the voucher number `CSH-000001`.
    pub number: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub amount_cents: i64,
    pub paid_from: PaidFrom,
}
