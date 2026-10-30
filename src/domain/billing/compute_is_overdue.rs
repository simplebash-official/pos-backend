use super::invoice_status::InvoiceStatus;

use chrono::{NaiveDate, Utc};

/// Whether an invoice should be flagged overdue: still awaiting (full)
/// payment and its `due_date` has passed. Computed fresh on every read
/// (`InvoiceDocument::into_invoice`) rather than stored, since it's a pure
/// function of two fields that are already persisted.
#[must_use]
pub fn compute_is_overdue(status: InvoiceStatus, due_date: Option<&str>) -> bool {
    if !matches!(
        status,
        InvoiceStatus::Pending | InvoiceStatus::PartiallyPaid
    ) {
        return false;
    }
    let Some(due_date) = due_date else {
        return false;
    };
    match NaiveDate::parse_from_str(due_date, "%Y-%m-%d") {
        Ok(due) => due < Utc::now().date_naive(),
        Err(_) => false,
    }
}
