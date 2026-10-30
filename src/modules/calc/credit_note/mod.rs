// The Return / Credit note screen's live figures: what each returned unit is worth back, the
// return and exchange subtotals, the net, how much of it goes back as cash (capped at what was paid
// on the invoice), where that cash goes, and whether the return window has passed. The invoice is
// read; nothing is written. The maths is `core::calculations`, the same code the credit note is
// created with, so the refund shown is the refund issued.

mod calc_credit_note_reply;
mod calc_credit_note_request;
mod calc_exchange_line;
mod calc_no_receipt_line;
mod calc_refund_leg;
mod calc_returned_line;
mod calc_returned_line_reply;
mod compute;
mod invoice_context;

pub use calc_credit_note_reply::CalcCreditNoteReply;
pub use calc_credit_note_request::CalcCreditNoteRequest;
pub use calc_exchange_line::CalcExchangeLine;
pub use calc_no_receipt_line::CalcNoReceiptLine;
pub use calc_refund_leg::CalcRefundLeg;
pub use calc_returned_line::CalcReturnedLine;
pub use calc_returned_line_reply::CalcReturnedLineReply;
pub use compute::compute;
pub use invoice_context::InvoiceContext;
