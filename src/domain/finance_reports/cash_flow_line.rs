use serde::Serialize;
use utoipa::ToSchema;

/// What moved through an account for one reason.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashFlowLine {
    /// A fixed word the screen turns into plain language: `sales`, `refunds`, `other_income`,
    /// `cash_added`, `paid_out`, `owner_withdrawals`, `cash_to_bank_or_safe`, `bank_deposits`,
    /// `bank_withdrawals`, `expense_payments`.
    pub code: String,
    pub in_cents: i64,
    pub out_cents: i64,
}
