use super::merge_class::MergeClass;
use super::phase::Phase;

#[derive(Debug)]
pub struct ResourceSpec {
    /// Wire name (camelCase), e.g. `stockMovements`.
    pub name: &'static str,
    /// SQLite table == Mongo collection.
    pub table: &'static str,
    pub class: MergeClass,
    /// Has a `deleted_at` column (deletes are tombstones).
    pub soft_delete: bool,
    /// snake_case columns never taken from a payload.
    pub derived: &'static [&'static str],
    /// Columns never put on the wire (none in P3a).
    pub local_only: &'static [&'static str],
    /// Apply order, ascending: parents before children.
    pub order: u8,
    pub phase: Phase,
}
