//! Mongo and SQLite access for the `invoices`, `payments`, and `credit_notes`
//! collections. Same never-interpret-a-miss-as-an-error convention as every
//! other repository in this codebase — `service` decides what a missing row
//! means.

mod billing_shared;
mod billing_stats;
pub(crate) mod credit_notes;
pub(crate) mod invoice;
mod invoice_lookups;
mod invoice_writes;
mod payment_records;

pub(crate) use billing_shared::credit_note_from_sqlite_row;
pub(crate) use billing_stats::aggregate_stats;
pub(crate) use payment_records::count_payments_for_invoice;

pub(crate) use billing_shared::invoice_from_sqlite_row;
pub(crate) use invoice_lookups::find_invoice_by_id_or_key;
pub(crate) use invoice_lookups::find_invoice_by_key;
pub(crate) use invoice_lookups::find_session_invoices_and_payments;
pub(crate) use invoice_writes::insert_invoice;
pub(crate) use payment_records::find_payment_by_key;
pub(crate) use payment_records::insert_payment;

pub(crate) use invoice_lookups::list_invoices;
pub(crate) use payment_records::list_payments_for_invoice;

pub(crate) use payment_records::payment_totals_by_method;

pub(crate) use invoice_writes::update_invoice_status;
