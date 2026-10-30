//! The Return / Credit note screen's live figures: what each returned unit is worth back, the
//! return and exchange subtotals, the net, how much of it goes back as cash (capped at what was paid
//! on the invoice), where that cash goes, and whether the return window has passed. The invoice is
//! read; nothing is written. The maths is `core::calculations`, the same code the credit note is
//! created with, so the refund shown is the refund issued.

mod calc_credit_note;

pub use calc_credit_note::CalcCreditNoteReply;
pub use calc_credit_note::CalcCreditNoteRequest;
pub use calc_credit_note::CalcExchangeLine;
pub use calc_credit_note::CalcNoReceiptLine;
pub use calc_credit_note::CalcRefundLeg;
pub use calc_credit_note::CalcReturnedLine;
pub use calc_credit_note::CalcReturnedLineReply;
pub use calc_credit_note::InvoiceContext;
pub use calc_credit_note::compute;
