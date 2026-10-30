//! Device-local sync API, called only by the desktop shell's sync agent with a
//! service token (`SyncAgent`). It exposes this install's sync state, its
//! outbox (what to push), the applier (what was pulled), cloud-reserved number
//! blocks and the reviewable conflict list, plus `GET /local/events`, which
//! tells the agent the moment a local write may have queued something to
//! upload. These routes exist only on SQLite (desktop) deployments; on the
//! cloud they answer 404.

mod sync_routes_local;

pub use sync_routes_local::router;
