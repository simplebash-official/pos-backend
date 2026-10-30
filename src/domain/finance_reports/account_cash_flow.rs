use super::cash_flow_line::CashFlowLine;
use crate::domain::accounts::AccountKind;

use serde::Serialize;
use utoipa::ToSchema;

/// Money in and out of one account over the period.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountCashFlow {
    pub kind: AccountKind,
    pub in_cents: i64,
    pub out_cents: i64,
    pub net_cents: i64,
    pub lines: Vec<CashFlowLine>,
}
