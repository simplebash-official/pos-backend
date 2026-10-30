// Device-local sync API, called only by the desktop shell's sync agent with a
// service token (`SyncAgent`). It exposes this install's sync state, its
// outbox (what to push), the applier (what was pulled), cloud-reserved number
// blocks and the reviewable conflict list, plus `GET /local/events`, which
// tells the agent the moment a local write may have queued something to
// upload. These routes exist only on SQLite (desktop) deployments; on the
// cloud they answer 404.

mod ack_outbox;
mod apply_changes;
mod enable_sync;
mod get_outbox;
mod get_pending;
mod get_state;
mod list_conflicts;
mod local_events;
mod resolve_conflict;
mod router;
mod seed_outbox;
mod sku_prefixes;
mod store_block;
mod update_state;

pub use router::router;
