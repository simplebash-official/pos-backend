//! Idempotent start-up seeding of reference and sample data. Seeding never overwrites existing rows.

pub mod admin;
pub mod api_key;
pub mod cash_categories;
pub mod customers;
pub mod inventory;
pub mod providers;
mod seeds_items;
pub mod suppliers;

pub use seeds_items::SeedSummary;
pub use seeds_items::seed_all;
