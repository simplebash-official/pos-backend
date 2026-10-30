// Pure business types for supplier credits: an amount a supplier takes off what the shop owes,
// because goods were short or damaged or the price was wrong. No I/O, no Mongo/Axum types beyond
// serde/utoipa derives.
//
// A credit lowers the supplier's balance and, when it names a receipt, that receipt's amount
// due. It moves no stock: the shop receives only the units it accepts. A wrong credit is voided
// by a reversing entry, never edited or deleted.

mod create_supplier_credit_request;
mod credit_reason;
mod supplier_credit;
mod supplier_credit_list_query;
mod supplier_credit_list_response;

pub use create_supplier_credit_request::CreateSupplierCreditRequest;
pub use credit_reason::CreditReason;
pub use supplier_credit::SupplierCredit;
pub use supplier_credit_list_query::SupplierCreditListQuery;
pub use supplier_credit_list_response::SupplierCreditListResponse;
