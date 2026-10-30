//! Business rules for quotations: validation, server-side totals, numbering
//! and the frozen shop-profile snapshot. A quotation never calls into stock,
//! payments, invoices or reports — it is a record of an offer, nothing more.

mod quotations_service;

pub use quotations_service::cancel_quotation;
pub use quotations_service::create_quotation;
pub use quotations_service::fulfill_quotation;
pub use quotations_service::get_quotation;
pub(crate) use quotations_service::get_quotation_for_print;
pub(crate) use quotations_service::hydrate_sync_documents;
pub use quotations_service::list_quotations;
