// HTTP layer for the payee directory.

mod create_payee;
mod list_payees;
mod router;
mod update_payee;

pub use router::router;
