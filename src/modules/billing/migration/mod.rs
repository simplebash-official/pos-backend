//! The one-time move that marks every repair job and print job already billed by an invoice with
//! that invoice: SQLite at startup (`migrate_sqlite`), MongoDB with `cargo run --bin
//! migrate_ticket_billing`, old backups through the same pure rule (`plan`).

mod ticket_billing;
mod ticket_billing_migration;

pub use ticket_billing::BilledTicket;
pub use ticket_billing::InvoiceFacts;
pub use ticket_billing::InvoiceLine;
pub use ticket_billing::Stamp;
pub use ticket_billing::TicketKind;
pub use ticket_billing::plan;
pub use ticket_billing_migration::Report;
pub use ticket_billing_migration::migrate_mongo;
pub use ticket_billing_migration::migrate_sqlite;
