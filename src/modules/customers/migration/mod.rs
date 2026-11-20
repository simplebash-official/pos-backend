//! The one-time move that gives every repair job and print job saved without a customer link
//! one: SQLite at startup (`migrate_sqlite`), MongoDB with `cargo run --bin
//! migrate_ticket_customers`, old backups through the same pure rule (`plan`).

mod ticket_customers;
mod ticket_customers_migration;

pub use ticket_customers::KnownCustomer;
pub use ticket_customers::Link;
pub use ticket_customers::NewCustomer;
pub use ticket_customers::Plan;
pub use ticket_customers::TicketKind;
pub use ticket_customers::UnlinkedTicket;
pub use ticket_customers::migrated_customer_key;
pub use ticket_customers::phone_identity;
pub use ticket_customers::plan;
pub use ticket_customers_migration::Report;
pub use ticket_customers_migration::migrate_mongo;
pub use ticket_customers_migration::migrate_sqlite;
