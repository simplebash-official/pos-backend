use super::account_cash_flow::AccountCashFlow;

use serde::Serialize;
use utoipa::ToSchema;

/// Response of `GET /finance-reports/cash-flow`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashFlowResponse {
    pub from: String,
    pub to: String,
    pub accounts: Vec<AccountCashFlow>,
}
