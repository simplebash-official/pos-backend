// Mongo-only query/persistence layer for the reports module. Performs aggregation
// pipelines across collections (`invoices`, `products`, `repairs`, `print_jobs`, `credit_notes`).

pub(crate) mod commissions;
pub(crate) mod inventory_valuation;
pub(crate) mod receivables;
pub(crate) mod refunds;
pub(crate) mod sales;
