// Mongo and SQLite access for the `invoices`, `payments`, and `credit_notes`
// collections. Same never-interpret-a-miss-as-an-error convention as every
// other repository in this codebase — `service` decides what a missing row
// means.

mod aggregate_stats;
mod count_payments_for_invoice;
mod credit_note_from_sqlite_row;
pub(crate) mod credit_notes;
mod find_invoice_by_id;
mod find_invoice_by_id_or_key;
mod find_invoice_by_key;
mod find_payment_by_key;
mod find_session_invoices_and_payments;
mod insert_invoice;
mod insert_payment;
pub(crate) mod invoice;
mod invoice_from_sqlite_row;
mod invoices;
mod list_invoices;
mod list_payments_for_invoice;
mod payment_from_sqlite_row;
mod payment_totals_by_method;
mod payments;
mod stats_aggregate_result;
mod update_invoice_status;

pub(crate) use aggregate_stats::aggregate_stats;
pub(crate) use count_payments_for_invoice::count_payments_for_invoice;
pub(crate) use credit_note_from_sqlite_row::credit_note_from_sqlite_row;

pub(crate) use find_invoice_by_id_or_key::find_invoice_by_id_or_key;
pub(crate) use find_invoice_by_key::find_invoice_by_key;
pub(crate) use find_payment_by_key::find_payment_by_key;
pub(crate) use find_session_invoices_and_payments::find_session_invoices_and_payments;
pub(crate) use insert_invoice::insert_invoice;
pub(crate) use insert_payment::insert_payment;
pub(crate) use invoice_from_sqlite_row::invoice_from_sqlite_row;

pub(crate) use list_invoices::list_invoices;
pub(crate) use list_payments_for_invoice::list_payments_for_invoice;

pub(crate) use payment_totals_by_method::payment_totals_by_method;

pub(crate) use update_invoice_status::update_invoice_status;
