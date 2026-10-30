// Device-side change capture for sync v2 (SQLite only).
//
// Every synced table gets AFTER INSERT / AFTER UPDATE triggers that enqueue the
// changed row into `sync_outbox` (coalesced per `(resource, key)`) and record
// its merge metadata in `sync_row_meta`. Triggers are always installed but do
// nothing unless `sync_state.capture_enabled = 1`, so an unlinked install pays
// nothing; they also stay silent while `sync_state.applying = 1`, which is what
// keeps the applier from echoing remote changes back into the outbox.
//
// Deliberately NOT here: reading or applying changes (`modules::sync`).

mod ensure_state;
mod install_triggers;

pub use ensure_state::ensure_state;
pub use install_triggers::install_triggers;
