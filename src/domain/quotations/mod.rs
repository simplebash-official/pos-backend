//! API-facing types for the quotations feature (priced offers given to a
//! customer before — and independent of — any sale). No I/O and no Mongo
//! types: the stored shape lives in `modules::quotations::model`.

mod domain_quotations;

pub use domain_quotations::CreateQuotationItem;
pub use domain_quotations::CreateQuotationRequest;
pub use domain_quotations::Quotation;
pub use domain_quotations::QuotationCustomer;
pub use domain_quotations::QuotationItem;
pub use domain_quotations::QuotationListQuery;
pub use domain_quotations::QuotationListResponse;
pub use domain_quotations::STATUS_ACTIVE;
pub use domain_quotations::STATUS_CANCELLED;
pub use domain_quotations::STATUS_FULFILLED;
pub use domain_quotations::SyncQuotation;
