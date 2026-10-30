#![forbid(unsafe_code)]

use super::ConflictKind;

#[derive(Debug, Clone, PartialEq, Eq)]
/// What to do with one incoming change.
pub enum Decision {
    /// The change wins (or the row is new): write it.
    Apply,
    /// Already applied (identical write); nothing to do.
    Duplicate,
    /// The incoming change lost; keep the local row and store the loser.
    KeepLocal { loser: ConflictKind },
    /// Refuse the change with a stable reason string.
    Reject { reason: &'static str },
    /// Lifecycle class: merge only the lifecycle columns (`merge_lifecycle`).
    MergeLifecycle,
}
