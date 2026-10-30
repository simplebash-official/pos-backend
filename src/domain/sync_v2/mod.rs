//! Wire and hand-off types of sync v2 (two-way device <-> cloud sync). Pure data:
//! no I/O, no framework types beyond serde/utoipa derives. The wire shape is
//! pinned by the sync v2 spec; PKG-2 (SQLite device), PKG-3 (Mongo cloud) and
//! PKG-4 (desktop agent) all compile against these definitions, so change them
//! only together.

mod domain_sync_v2;

pub use domain_sync_v2::AckStatus;
pub use domain_sync_v2::ApplyMode;
pub use domain_sync_v2::ApplyOrigin;
pub use domain_sync_v2::ApplyOutcome;
pub use domain_sync_v2::ApplyRequest;
pub use domain_sync_v2::ApplyResponse;
pub use domain_sync_v2::ChangeAck;
pub use domain_sync_v2::ChangeOp;
pub use domain_sync_v2::ChangeRecord;
pub use domain_sync_v2::ConflictKind;
pub use domain_sync_v2::DerivedScope;
pub use domain_sync_v2::NewConflict;
pub use domain_sync_v2::PullResponse;
pub use domain_sync_v2::PulledChange;
pub use domain_sync_v2::PushRequest;
pub use domain_sync_v2::PushResponse;
pub use domain_sync_v2::SnapshotResponse;
