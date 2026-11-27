//! The one-time moves of old print jobs, each run on SQLite at startup, on MongoDB by a tool in
//! `src/bin`, and on old backups by `modules::backup`:
//!
//! - jobs saved as `in_progress`, a status that was never meant to exist, move to `in_repair`
//!   (shown as "In production"): `migrate_sqlite`, `migrate_print_job_status`;
//! - jobs with one quoted price and one material cost move to lines: `migrate_items_sqlite`,
//!   `migrate_print_job_items`.

mod legacy_print_job;
mod print_job_items_migration;
mod print_job_status_migration;

pub use legacy_print_job::LegacyPrintJob;
pub use legacy_print_job::MIGRATED_MATERIALS_NAME;
pub use legacy_print_job::MigratedItems;
pub use legacy_print_job::legacy_items;
pub use print_job_items_migration::ItemsReport;
pub use print_job_items_migration::migrate_items_mongo;
pub use print_job_items_migration::migrate_items_sqlite;
pub use print_job_status_migration::CURRENT_STATUS;
pub use print_job_status_migration::LEGACY_STATUS;
pub use print_job_status_migration::Report;
pub use print_job_status_migration::migrate_mongo;
pub use print_job_status_migration::migrate_sqlite;
