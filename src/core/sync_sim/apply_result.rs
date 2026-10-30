/// Result of applying one change to a replica.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyResult {
    Applied,
    Duplicate,
    KeepLocal,
    Rejected(&'static str),
}
