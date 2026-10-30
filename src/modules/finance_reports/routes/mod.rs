// HTTP layer for the money reports. Read-only; every route needs `reports:view` (Admin and Manager).

mod cash_flow;
mod expense_breakdown;
mod payables;
mod period;
mod period_query;
mod profit_loss;
mod router;

pub use router::router;
