use super::invoice::Invoice;
use super::payment_record::PaymentRecord;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `POST /billing/sales`. `warnings` surfaces any sub-step
/// that failed after the invoice/payment were already durably recorded
/// (stock decrement, ticket status update, customer balance update) — see
/// D4 in the migration plan for why these never fail the whole request.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompleteSaleResponse {
    /// Created invoice record.
    pub invoice: Invoice,
    /// Recorded payment transactions for this sale.
    pub payments: Vec<PaymentRecord>,
    /// Non-fatal warnings encountered during post-sale processing.
    pub warnings: Vec<String>,
}
