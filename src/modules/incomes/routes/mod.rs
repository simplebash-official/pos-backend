// HTTP layer for the income ledger.

mod create_income;
mod get_income;
mod get_income_stats;
mod list_incomes;
mod router;

pub use router::router;
