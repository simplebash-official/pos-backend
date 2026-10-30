use super::payable_bill::PayableBill;
use super::payable_supplier::PayableSupplier;

use serde::Serialize;
use utoipa::ToSchema;

/// Response of `GET /finance-reports/payables`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PayablesResponse {
    /// `YYYY-MM-DD` (UTC) the figures are as of.
    pub as_of: String,
    /// Bills not paid in full, soonest due first; those without a due date last.
    pub bills: Vec<PayableBill>,
    pub bills_cents: i64,
    pub overdue_cents: i64,
    /// Suppliers with a balance owed, biggest first. Suppliers without an opening balance are not listed
    /// (their balance is not known, see the Balances screen).
    pub suppliers: Vec<PayableSupplier>,
    pub suppliers_cents: i64,
    pub total_cents: i64,
}
