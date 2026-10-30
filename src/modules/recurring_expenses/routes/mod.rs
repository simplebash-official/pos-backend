// HTTP layer for regular bills.

mod create_template;
mod get_period_status;
mod list_bills;
mod router;
mod update_template;

pub use router::router;
