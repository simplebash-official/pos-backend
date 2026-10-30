//! Device-local sync bookkeeping (SQLite): the single `sync_state` row, enabling
//! capture (opening-balance ledger migration + outbox seeding), acknowledging
//! pushed outbox rows and the reviewable conflict list. Nothing here talks to
//! the cloud - the desktop shell's sync agent drives it through the local
//! `/api/sync/*` routes.

mod sync_state;

pub use sync_state::ack_outbox;
pub use sync_state::enable;
pub use sync_state::get_state;
pub use sync_state::list_conflicts;
pub use sync_state::migrate_opening_balances;
pub(crate) use sync_state::pool_of;
pub use sync_state::resolve_conflict;
pub use sync_state::seed_outbox;
pub use sync_state::update_state;
