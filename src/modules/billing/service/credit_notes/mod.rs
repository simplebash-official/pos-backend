// Business rules and orchestration for credit notes: returns, refunds
// (cashback), and replacements/exchanges. Gated by `CurrentUser` +
// `billing:write` permission (return-window override and no-receipt returns
// additionally require the caller to hold `Role::Admin`). A credit note is
// created once as a single atomic write — this codebase has no edit-in-place
// endpoint for any billing document, so there is deliberately no way to move
// one out of `AwaitingResolution` other than `void_credit_note`.

mod create_credit_note;
mod get_credit_note;
mod hydrate_sync_documents;
mod invoice_payment_legs;
mod line_hint;
mod line_match;
mod list_credit_notes;
mod void_credit_note;

pub use create_credit_note::create_credit_note;
pub use get_credit_note::get_credit_note;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use invoice_payment_legs::invoice_payment_legs;
pub(crate) use line_hint::LineHint;
pub(crate) use line_match::line_match;
pub use list_credit_notes::list_credit_notes;
pub use void_credit_note::void_credit_note;
