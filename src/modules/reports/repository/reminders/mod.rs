// Mongo queries for the dashboard reminders feed: unpaid credit invoices and
// open repair/print jobs whose due / promised date is overdue or coming up.
//
// Kept deliberately simple — a `find` per source plus one batched payments
// lookup, then all the date math in Rust — because the set of open credit
// invoices + open jobs a shop has at any moment is small. Mirrors the style
// of `receivables::list_outstanding_invoices`.

mod list_reminders;
mod reminders_raw;

pub(crate) use list_reminders::list_reminders;
