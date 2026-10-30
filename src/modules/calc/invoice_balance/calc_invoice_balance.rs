use crate::core::calculations::invoice_paid_and_balance;
use crate::domain::billing::{Invoice, PaymentRecord};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcInvoiceBalanceReply {
    /// Everything paid against the invoice (checkout deposit and later payments).
    pub paid_cents: i64,
    /// What is still owed: nothing for a sale paid at the till.
    pub balance_due_cents: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcInvoiceBalanceRequest {
    pub invoice_key: String,
}

#[must_use]
/// Computes what is still owed on `invoice` after `payments`.
pub fn compute(invoice: &Invoice, payments: &[PaymentRecord]) -> CalcInvoiceBalanceReply {
    let paid: i64 = payments.iter().map(|p| p.amount_cents).sum();
    let (paid_cents, balance_due_cents) = invoice_paid_and_balance(
        invoice.is_credit,
        invoice.total_cents,
        paid,
        invoice.refunded_cents,
    );
    CalcInvoiceBalanceReply {
        paid_cents,
        balance_due_cents,
    }
}
