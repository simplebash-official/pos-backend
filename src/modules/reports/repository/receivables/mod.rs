//! MongoDB aggregation queries for credit receivables and overdue balance tracking.

mod repository_receivables;

pub(crate) use repository_receivables::aggregate_outstanding_stats;
pub(crate) use repository_receivables::list_outstanding_invoices;
