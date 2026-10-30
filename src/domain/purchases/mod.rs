// Pure business types for the supplier purchase / stock-intake history
// feature — no I/O, no Mongo/Axum types beyond serde/utoipa derives. A `Purchase` is one product
// line of a `PurchaseReceipt` (`domain::purchase_receipts`); lines are created only through a receipt.

mod product_summary;
mod purchase;
mod purchase_list_query;
mod purchase_list_response;
mod supplier_summary;

pub use product_summary::ProductSummary;
pub use purchase::Purchase;
pub use purchase_list_query::PurchaseListQuery;
pub use purchase_list_response::PurchaseListResponse;
pub use supplier_summary::SupplierSummary;
