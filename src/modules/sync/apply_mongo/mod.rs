//! Cloud (MongoDB) side of applying a decided sync change: loading the row's
//! merge metadata, writing upserts / tombstones / lifecycle merges, and the two
//! resource-specific rules (categories fold their subcategories; serials detect
//! a double sale). The decision itself (`core::sync_merge::decide`) is made by
//! the caller; this module only performs and describes the write.
//!
//! Writes always go through the scoped handle, so a push can only touch the
//! tenant its device token names. Derived columns (stock, balances, refund
//! totals) are never overwritten here - `derived_mongo` recomputes them after
//! the batch.

mod apply_upsert;
mod sync_apply_mongo;
#[doc(hidden)]
pub mod test_support;

pub(crate) use apply_upsert::apply_upsert;
pub(crate) use sync_apply_mongo::ExistingRow;
pub(crate) use sync_apply_mongo::WriteOutcome;
pub(crate) use sync_apply_mongo::apply_delete;
pub(crate) use sync_apply_mongo::apply_lifecycle;
pub(crate) use sync_apply_mongo::load_existing;
pub(crate) use sync_apply_mongo::num_i64;
pub(crate) use sync_apply_mongo::required_i64;
pub(crate) use sync_apply_mongo::required_str;
