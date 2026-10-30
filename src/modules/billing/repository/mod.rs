//! Mongo and SQLite access for the `invoices`, `payments`, and `credit_notes`
//! collections. Same never-interpret-a-miss-as-an-error convention as every
//! other repository in this codebase — `service` decides what a missing row
//! means.

mod billing_repository_part1;
mod billing_repository_part2;
pub(crate) mod credit_notes;
pub(crate) mod invoice;

pub(crate) use billing_repository_part1::aggregate_stats;
pub(crate) use billing_repository_part1::count_payments_for_invoice;
pub(crate) use billing_repository_part1::credit_note_from_sqlite_row;

pub(crate) use billing_repository_part1::find_invoice_by_id_or_key;
pub(crate) use billing_repository_part1::find_invoice_by_key;
pub(crate) use billing_repository_part1::find_payment_by_key;
pub(crate) use billing_repository_part1::find_session_invoices_and_payments;
pub(crate) use billing_repository_part1::insert_invoice;
pub(crate) use billing_repository_part1::insert_payment;
pub(crate) use billing_repository_part1::invoice_from_sqlite_row;

pub(crate) use billing_repository_part1::list_invoices;
pub(crate) use billing_repository_part1::list_payments_for_invoice;

pub(crate) use billing_repository_part1::payment_totals_by_method;

pub(crate) use billing_repository_part2::update_invoice_status;
