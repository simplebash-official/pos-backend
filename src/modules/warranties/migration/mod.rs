//! The one-time move from the old warranty fields to warranty records: SQLite at startup
//! (`migrate_sqlite`), MongoDB with `cargo run --bin migrate_warranties`, old backups through
//! the same pure conversions.

mod legacy_warranty;
mod warranty_migration;

pub use legacy_warranty::LegacyInvoice;
pub use legacy_warranty::LegacySoldUnit;
pub use legacy_warranty::MigratedWarranty;
pub use legacy_warranty::legacy_profile;
pub use legacy_warranty::legacy_warranty;
pub use legacy_warranty::migrated_key;
pub use warranty_migration::Report;
pub use warranty_migration::migrate_mongo;
pub use warranty_migration::migrate_sqlite;
