//! The one-time move from a repair job's two old amounts (`estimated_cost_cents`,
//! `material_cost_cents`) to its items: SQLite at startup (`migrate_sqlite`), MongoDB with
//! `cargo run --bin migrate_repair_items`, old backups through the same pure conversion.

mod legacy_repair;
mod repair_migration;

pub use legacy_repair::MIGRATED_MATERIALS_NAME;
pub use legacy_repair::MigratedItems;
pub use legacy_repair::legacy_items;
pub use legacy_repair::migrated_item_key;
pub use repair_migration::Report;
pub use repair_migration::migrate_mongo;
pub use repair_migration::migrate_sqlite;
