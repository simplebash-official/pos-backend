//! The one-time catch-up for the money ledgers and shop rules, which earlier builds could not turn
//! into sync records: a device queues its own rows again at startup (`migrate_sqlite`), and the
//! cloud publishes its rows to every shop's change feed with `cargo run --bin
//! migrate_sync_ledger_backfill` (`migrate_mongo`).
//!
//! A second catch-up numbers the cloud accounts stored without a version, which no device could
//! receive: `cargo run --bin migrate_user_versions` (`migrate_user_versions`).

mod ledger_backfill;
mod ledger_requeue;
mod ledger_resources;
mod user_version_backfill;

pub use ledger_backfill::Report;
pub use ledger_backfill::migrate_mongo;
pub use ledger_requeue::migrate_sqlite;
pub use user_version_backfill::migrate_user_versions;
