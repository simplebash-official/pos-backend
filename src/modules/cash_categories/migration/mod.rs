// One-time MongoDB move of `cash_movements` to typed movements with categories, the
// cloud twin of `clients::sqlite::migrate_cash_movements` (same mapping, same
// deterministic category keys, so a desktop and the cloud agree and sync merges instead of
// duplicating). Run by `cargo run --bin migrate_cash_movements`; idempotent, since only
// documents that still carry the old `reason` field are touched. Also makes sure every
// shop has the built-in categories the migrated movements point at.

mod migrate_mongo;
mod report;

pub use migrate_mongo::migrate_mongo;
pub use report::Report;
