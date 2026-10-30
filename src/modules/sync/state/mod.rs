// Device-local sync bookkeeping (SQLite): the single `sync_state` row, enabling
// capture (opening-balance ledger migration + outbox seeding), acknowledging
// pushed outbox rows and the reviewable conflict list. Nothing here talks to
// the cloud - the desktop shell's sync agent drives it through the local
// `/api/sync/*` routes.

mod ack_outbox;
mod enable;
mod get_state;
mod list_conflicts;
mod migrate_opening_balances;
mod pool_of;
mod resolve_conflict;
mod seed_outbox;
mod update_state;

pub use ack_outbox::ack_outbox;
pub use enable::enable;
pub use get_state::get_state;
pub use list_conflicts::list_conflicts;
pub use migrate_opening_balances::migrate_opening_balances;
pub(crate) use pool_of::pool_of;
pub use resolve_conflict::resolve_conflict;
pub use seed_outbox::seed_outbox;
pub use update_state::update_state;
