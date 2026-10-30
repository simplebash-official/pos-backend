// Realtime "something changed" notifications for sync, so replicas react in
// milliseconds instead of waiting for their next poll. Notifications carry no
// record data (only a sequence number, the resource name and who wrote it):
// receivers still read the change through the normal pull or REST endpoints,
// which keeps permissions and payload shapes exactly where they are.
//
// Two independent channels live here:
//
// - Cloud (`TENANT_MODE=multi`): `run_watcher` watches inserts into the
//   tenant-scoped `sync_changes` log and fans each one out to that tenant's
//   subscribers of `GET /api/sync/events` (devices and browser tabs). Every
//   instance runs its own watcher, so subscribers can be served by any
//   instance without a lease.
// - Desktop (SQLite): `local_write_tick` marks every successful write request,
//   and `GET /api/sync/local/events` turns those marks into a stream for the
//   desktop shell's sync agent, which then uploads at once.
//
// Both use process-wide registries rather than `AppState` fields: they are
// keyed by tenant (cloud) or are per-process by nature (desktop).

mod cloud_events;
mod heartbeat;
mod keep_alive;
mod live_event;
mod live_event_of;
mod local_events;
mod local_write_tick;
mod local_writes;
mod publish;
mod run_watcher;
mod subscribe;
mod tenants;
#[cfg(test)]
mod tests;

pub use cloud_events::cloud_events;
pub use heartbeat::HEARTBEAT;
pub use keep_alive::keep_alive;
pub use live_event::LiveEvent;
pub use local_events::local_events;
pub use local_write_tick::local_write_tick;
pub use publish::publish;
pub use run_watcher::run_watcher;
pub use subscribe::subscribe;
