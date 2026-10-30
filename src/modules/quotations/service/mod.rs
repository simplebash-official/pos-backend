// Business rules for quotations: validation, server-side totals, numbering
// and the frozen shop-profile snapshot. A quotation never calls into stock,
// payments, invoices or reports — it is a record of an offer, nothing more.

mod cancel_quotation;
mod create_quotation;
mod find_document;
mod fulfill_quotation;
mod get_quotation;
mod get_quotation_for_print;
mod hydrate_sync_documents;
mod list_quotations;
mod today;

pub use cancel_quotation::cancel_quotation;
pub use create_quotation::create_quotation;
pub use fulfill_quotation::fulfill_quotation;
pub use get_quotation::get_quotation;
pub(crate) use get_quotation_for_print::get_quotation_for_print;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_quotations::list_quotations;
