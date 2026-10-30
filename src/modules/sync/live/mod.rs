//! Realtime "something changed" notifications for sync, so replicas react in
//! milliseconds instead of waiting for their next poll. Notifications carry no
//! record data (only a sequence number, the resource name and who wrote it):
//! receivers still read the change through the normal pull or REST endpoints,
//! which keeps permissions and payload shapes exactly where they are.
//!
//! Two independent channels live here:
//!
//! - Cloud (`TENANT_MODE=multi`): `run_watcher` watches inserts into the
//!   tenant-scoped `sync_changes` log and fans each one out to that tenant's
//!   subscribers of `GET /api/sync/events` (devices and browser tabs). Every
//!   instance runs its own watcher, so subscribers can be served by any
//!   instance without a lease.
//! - Desktop (SQLite): `local_write_tick` marks every successful write request,
//!   and `GET /api/sync/local/events` turns those marks into a stream for the
//!   desktop shell's sync agent, which then uploads at once.
//!
//! Both use process-wide registries rather than `AppState` fields: they are
//! keyed by tenant (cloud) or are per-process by nature (desktop).

mod sync_live;
#[cfg(test)]
mod tests;

pub use sync_live::HEARTBEAT;
pub use sync_live::LiveEvent;
pub use sync_live::cloud_events;
pub use sync_live::keep_alive;
pub use sync_live::local_events;
pub use sync_live::local_write_tick;
pub use sync_live::publish;
pub use sync_live::run_watcher;
pub use sync_live::subscribe;
