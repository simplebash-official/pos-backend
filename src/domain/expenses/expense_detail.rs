use super::expense_payment::ExpensePayment;
use super::expense_row::ExpenseRow;

use serde::Serialize;
use utoipa::ToSchema;

/// A ledger expense with its payments.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseDetail {
    pub expense: ExpenseRow,
    pub payments: Vec<ExpensePayment>,
}
