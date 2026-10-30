// HTTP layer for billing: sale completion, invoice/payment reads, payment
// recording, cancellation, and on-demand document (PDF) fetching. The
// module-status stub at `GET /` is unrelated to any of that and stays as
// the intentionally-public placeholder it always was (see `app.rs`'s
// `PUBLIC_ROUTES` note) — `/invoices` is the real "list" entry point, not
// `/`, since billing's root has no natural single-collection meaning the
// way `suppliers`'/`customers`' root does.

mod close_invoice;
mod complete_sale;
mod create_credit_note;
mod design_status_params;
mod document_query_params;
mod get_credit_note;
mod get_credit_note_document;
mod get_credit_note_document_design;
mod get_invoice;
mod get_invoice_document;
mod get_invoice_document_design;
mod get_invoice_stats;
mod invoice_variant;
mod list_credit_notes;
mod list_invoices;
mod list_payments;
mod record_payment;
mod require_credit_note_type;
mod router;
mod status;
mod void_credit_note;
mod void_invoice;

pub use router::router;
