//! Derived-field recompute for the SQLite (device) engine. Sync never carries
//! these columns: stock, customer balances and an invoice's refund totals /
//! credit-sale status are recomputed from the ledger rows (movements, invoices,
//! payments, credit notes) after every applied batch, so replaying the same
//! batch - or merging concurrent edits - always converges on the ledger truth.
//!
//! Runs inside the applier's transaction (`sync_state.applying = 1`), so nothing
//! it writes is enqueued to the outbox. Formulas are pinned in the sync v2 spec
//! section 1.3; the Mongo engine implements identical semantics.

mod sync_derived_sqlite;

pub use sync_derived_sqlite::recompute;
pub use sync_derived_sqlite::store_conflicts;
