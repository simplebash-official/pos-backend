// HTTP layer for the live calculations: each handler checks the same permission as the write it
// previews, makes one call into the pure engine and wraps the reply. Nothing is stored, so none of
// these count as writes (no idempotency key, no sync wake-up; see `app.rs`).

mod calc_cash_count;
mod calc_cash_movement;
mod calc_checkout;
mod calc_credit_note;
mod calc_earnings;
mod calc_invoice_balance;
mod calc_quotation;
mod calc_receipt;
mod calc_settlement;
mod calc_shares;
mod calc_shift_count;
mod calc_shift_totals;
mod router;

pub use router::router;
