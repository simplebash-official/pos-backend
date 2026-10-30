use super::calc_invoice_balance_reply::CalcInvoiceBalanceReply;
use crate::core::calculations::invoice_paid_and_balance;
use crate::domain::billing::{Invoice, PaymentRecord};

#[must_use]
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
