//! The one-time move of print jobs saved as `in_progress`, a status that was never meant to
//! exist, to `in_repair` (shown as "In production"): SQLite at startup (`migrate_sqlite`),
//! MongoDB with `cargo run --bin migrate_print_job_status`, old backups through the same two
//! constants.

mod print_job_status_migration;

pub use print_job_status_migration::CURRENT_STATUS;
pub use print_job_status_migration::LEGACY_STATUS;
pub use print_job_status_migration::Report;
pub use print_job_status_migration::migrate_mongo;
pub use print_job_status_migration::migrate_sqlite;
