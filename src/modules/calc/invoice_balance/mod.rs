//! What a customer has paid on an invoice and what they still owe, for the invoice screen. The rule
//! is `core::calculations::invoice_paid_and_balance`, the same one the printed invoice and the
//! payment check use. The handler reads the invoice and its payments.

mod calc_invoice_balance;

pub use calc_invoice_balance::CalcInvoiceBalanceReply;
pub use calc_invoice_balance::CalcInvoiceBalanceRequest;
pub use calc_invoice_balance::compute;
