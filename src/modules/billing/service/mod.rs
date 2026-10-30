// Business rules and orchestration for the billing module. `sale` owns
// complete-sale, void, and close (the money-moving flows, D3/D4/D8 in the
// migration plan); `payments` owns standalone payment recording (partial
// credit repayments); `credit_notes` owns returns/refunds/exchanges;
// `print_payload` builds the Typst data contract consumed by
// `modules::documents::service`. Read paths (`get_invoice`/`list_invoices`)
// live directly here since they're straightforward lookups, not
// orchestration.

pub mod credit_notes;
mod get_billing_stats;
mod get_invoice;
mod get_session_billing_summary;
mod hydrate_sync_documents;
mod list_invoices;
pub mod payments;
pub(crate) mod print_payload;
pub mod sale;
mod session_billing_summary;

pub use get_billing_stats::get_billing_stats;
pub use get_invoice::get_invoice;
pub(crate) use get_session_billing_summary::get_session_billing_summary;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_invoices::list_invoices;
