// HTTP layer for quotations. Every route requires `quotations:manage`
// (Admin and Manager only), checked as the first line of each handler — a
// Staff token gets 403 here no matter what the UI shows.

mod cache_key_for;
mod cancel_quotation;
mod create_quotation;
mod get_quotation;
mod get_quotation_document;
mod get_quotation_document_design;
mod list_quotations;
mod router;

pub use router::router;
