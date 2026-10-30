use super::paid_from::PaidFrom;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /expenses`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateExpenseRequest {
    /// `YYYY-MM-DD`.
    pub expense_date: String,
    pub category_key: String,
    #[serde(default)]
    pub payee_key: Option<String>,
    #[serde(default)]
    pub payee_name: Option<String>,
    pub amount_cents: i64,
    /// `bank`, `card`, `online` (paid now, in full) or `unpaid` (a bill to pay later).
    pub paid_from: PaidFrom,
    /// `YYYY-MM-DD`; only for an unpaid bill.
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub reference_no: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    /// For a supplier payment paid on entry: the supplier receipts (invoices) it pays, and how
    /// much of it each.
    #[serde(default)]
    pub settlements: Vec<crate::domain::purchase_receipts::ReceiptSettlementRequest>,
}
