// Cloud (MongoDB) side of applying a decided sync change: loading the row's
// merge metadata, writing upserts / tombstones / lifecycle merges, and the two
// resource-specific rules (categories fold their subcategories; serials detect
// a double sale). The decision itself (`core::sync_merge::decide`) is made by
// the caller; this module only performs and describes the write.
//
// Writes always go through the scoped handle, so a push can only touch the
// tenant its device token names. Derived columns (stock, balances, refund
// totals) are never overwritten here - `derived_mongo` recomputes them after
// the batch.

mod apply_delete;
mod apply_lifecycle;
mod apply_upsert;
mod build_document;
mod existing_row;
mod load_existing;
mod millis;
mod note_scope;
mod num_i64;
#[doc(hidden)]
pub mod test_support;
mod write_outcome;

pub(crate) use apply_delete::apply_delete;
pub(crate) use apply_lifecycle::apply_lifecycle;
pub(crate) use apply_upsert::apply_upsert;
pub(crate) use existing_row::ExistingRow;
pub(crate) use load_existing::load_existing;
pub(crate) use num_i64::num_i64;
pub(crate) use write_outcome::WriteOutcome;
