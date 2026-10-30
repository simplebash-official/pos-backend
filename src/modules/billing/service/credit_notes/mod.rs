//! Business rules and orchestration for credit notes: returns, refunds
//! (cashback), and replacements/exchanges. Gated by `CurrentUser` +
//! `billing:write` permission (return-window override and no-receipt returns
//! additionally require the caller to hold `Role::Admin`). A credit note is
//! created once as a single atomic write — this codebase has no edit-in-place
//! endpoint for any billing document, so there is deliberately no way to move
//! one out of `AwaitingResolution` other than `void_credit_note`.

mod create_credit_note;
mod service_credit_notes;

pub use create_credit_note::create_credit_note;
pub(crate) use service_credit_notes::LineHint;
pub use service_credit_notes::get_credit_note;
pub(crate) use service_credit_notes::hydrate_sync_documents;
pub use service_credit_notes::invoice_payment_legs;
pub(crate) use service_credit_notes::line_match;
pub use service_credit_notes::list_credit_notes;
pub use service_credit_notes::void_credit_note;
