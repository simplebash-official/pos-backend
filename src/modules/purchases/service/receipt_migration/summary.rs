/// What a run changed (or, in a dry run, would change).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Summary {
    pub purchases: u64,
    pub receipts: u64,
    /// Purchase lines moved from a per-unit cost to a line total.
    pub line_totals: u64,
}
