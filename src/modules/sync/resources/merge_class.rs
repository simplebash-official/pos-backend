// Single source of truth for which resources sync, how each merges, which
// columns are derived (never taken from a payload) and the order changes are
// applied in. Shared by the device (SQLite) and cloud (Mongo) sides so both
// agree by construction. The older `SYNCABLE` list in `service.rs` keeps
// serving the legacy `/sync/changes` route and is intentionally left alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeClass {
    /// Rows are only ever inserted: union by `key`.
    AppendOnly,
    /// Whole-row last-writer-wins by `(updated_at, device_id)`.
    Lww,
    /// LWW on ordinary columns; `derived` columns are recomputed from ledgers.
    LwwWithDerived,
    /// Body immutable; lifecycle columns merge monotonically (invoices, credit notes, quotations).
    Lifecycle,
}
