// API-facing types for the quotations feature (priced offers given to a
// customer before — and independent of — any sale). No I/O and no Mongo
// types: the stored shape lives in `modules::quotations::model`.

mod create_quotation_item;
mod create_quotation_request;
mod quotation;
mod quotation_customer;
mod quotation_item;
mod quotation_list_query;
mod quotation_list_response;
mod status_active;
mod status_cancelled;
mod status_fulfilled;
mod sync_quotation;

pub use create_quotation_item::CreateQuotationItem;
pub use create_quotation_request::CreateQuotationRequest;
pub use quotation::Quotation;
pub use quotation_customer::QuotationCustomer;
pub use quotation_item::QuotationItem;
pub use quotation_list_query::QuotationListQuery;
pub use quotation_list_response::QuotationListResponse;
pub use status_active::STATUS_ACTIVE;
pub use status_cancelled::STATUS_CANCELLED;
pub use status_fulfilled::STATUS_FULFILLED;
pub use sync_quotation::SyncQuotation;
