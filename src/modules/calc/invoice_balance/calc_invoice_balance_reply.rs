use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcInvoiceBalanceReply {
    /// Everything paid against the invoice (checkout deposit and later payments).
    pub paid_cents: i64,
    /// What is still owed: nothing for a sale paid at the till.
    pub balance_due_cents: i64,
}
