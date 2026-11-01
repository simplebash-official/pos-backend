//! One-time MongoDB move of `drawer_state` from one balance per machine (`terminal_id`) to one
//! balance per user (`user_key`, the shift owner), the cloud twin of
//! `clients::sqlite::migrate_schema`'s `migrate_drawer_state_to_users` (same owner rule). Run by
//! `cargo run --bin migrate_drawer_by_user`; idempotent, since only rows that still carry
//! `terminal_id` are touched.

mod drawer_migration;

pub use drawer_migration::PlannedDrawer;
pub use drawer_migration::Report;
pub use drawer_migration::migrate_mongo;
