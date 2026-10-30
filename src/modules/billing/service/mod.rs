//! Business rules and orchestration for the billing module. `sale` owns
//! complete-sale, void, and close (the money-moving flows, D3/D4/D8 in the
//! migration plan); `payments` owns standalone payment recording (partial
//! credit repayments); `credit_notes` owns returns/refunds/exchanges;
//! `print_payload` builds the Typst data contract consumed by
//! `modules::documents::service`. Read paths (`get_invoice`/`list_invoices`)
//! live directly here since they're straightforward lookups, not
//! orchestration.

mod billing_service;
pub mod credit_notes;
pub mod payments;
pub(crate) mod print_payload;
pub mod sale;

pub use billing_service::get_billing_stats;
pub use billing_service::get_invoice;
pub(crate) use billing_service::get_session_billing_summary;
pub(crate) use billing_service::hydrate_sync_documents;
pub use billing_service::list_invoices;
