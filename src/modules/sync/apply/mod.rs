//! Device-side applier (SQLite): writes a batch of canonical change records into
//! the local database inside ONE `BEGIN IMMEDIATE` transaction.
//!
//!   1. `sync_state.applying = 1` (so the capture triggers stay silent - remote
//!      changes must never echo into the outbox), cleared again before COMMIT;
//!   2. per change (parents before children): `sync_merge::decide` against the
//!      row's `sync_row_meta`, then a generic column-driven upsert / tombstone;
//!   3. `derived_sqlite::recompute` for every entity the batch touched;
//!   4. the cursor advance, all in the same transaction.
//!
//! Any error rolls the whole batch back (flag and cursor included) and the
//! caller retries the same page, which is safe because applying is idempotent.
//!
//! The payload is the hydrated REST DTO (camelCase) plus the legacy `id`. It is
//! mapped onto the table's own columns (`pragma_table_info`), so DTO-only
//! enrichment (category names, `isOverdue`, supplier summaries...) is ignored
//! and derived columns are never taken from a payload.

mod apply_records_in_tx;
mod sync_apply;

pub use sync_apply::apply_batch;
pub use sync_apply::apply_records;
