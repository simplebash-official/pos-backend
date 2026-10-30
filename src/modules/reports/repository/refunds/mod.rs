//! MongoDB aggregation queries for credit notes and return refunds.

mod repository_refunds;

pub(crate) use repository_refunds::aggregate_refund_totals;
