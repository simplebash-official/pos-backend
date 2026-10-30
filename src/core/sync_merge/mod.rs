// Pure merge engine for sync v2: decides what to do with an incoming change
// given what the receiver already holds. No I/O; both the SQLite device
// applier and the Mongo cloud applier call `decide` so they always agree.
//
// Every rule is a total, order-independent function of (local state, incoming
// change), which is what makes replicas converge no matter the delivery order
// (see the convergence tests in `core::sync_sim`).

pub use crate::domain::sync_v2::{ConflictKind, NewConflict};
mod append_only_differs;
mod clamp_updated_at;
mod decide;
mod decide_append_only;
mod decision;
mod incoming_change;
mod is_meaningful_conflict;
mod lifecycle_merge;
mod lifecycle_rank;
mod merge_lifecycle;
mod meta_keys;
mod row_meta;
mod strip_for_compare;
#[cfg(test)]
mod tests;

pub use append_only_differs::append_only_differs;
pub use clamp_updated_at::clamp_updated_at;
pub use decide::decide;
pub use decide_append_only::decide_append_only;
pub use decision::Decision;
pub use incoming_change::IncomingChange;
pub use is_meaningful_conflict::is_meaningful_conflict;
pub use lifecycle_merge::LifecycleMerge;
pub use lifecycle_rank::lifecycle_rank;
pub use merge_lifecycle::merge_lifecycle;
pub use meta_keys::META_KEYS;
pub use row_meta::RowMeta;
pub use strip_for_compare::strip_for_compare;
