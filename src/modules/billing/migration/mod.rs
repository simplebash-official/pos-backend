//! The one-time move that marks every repair job and print job already billed by an invoice with
//! that invoice: SQLite at startup (`migrate_sqlite`), MongoDB with `cargo run --bin
//! migrate_ticket_billing`, old backups through the same pure rule (`plan`). Also the one-time copy
//! of a customer onto credit notes saved without one (`migrate_credit_note_customers_*`).

mod credit_note_customers_migration;
mod ticket_billing;
mod ticket_billing_migration;

pub use credit_note_customers_migration::CreditNoteCustomerReport;
pub use credit_note_customers_migration::migrate_credit_note_customers_mongo;
pub use credit_note_customers_migration::migrate_credit_note_customers_sqlite;
pub use ticket_billing::BilledTicket;
pub use ticket_billing::InvoiceFacts;
pub use ticket_billing::InvoiceLine;
pub use ticket_billing::Stamp;
pub use ticket_billing::TicketKind;
pub use ticket_billing::plan;
pub use ticket_billing_migration::Report;
pub use ticket_billing_migration::migrate_mongo;
pub use ticket_billing_migration::migrate_sqlite;
