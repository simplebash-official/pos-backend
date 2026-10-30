//! Pure business types for the supplier purchase / stock-intake history
//! feature — no I/O, no Mongo/Axum types beyond serde/utoipa derives. A `Purchase` is one product
//! line of a `PurchaseReceipt` (`domain::purchase_receipts`); lines are created only through a receipt.

mod domain_purchases;

pub use domain_purchases::ProductSummary;
pub use domain_purchases::Purchase;
pub use domain_purchases::PurchaseListQuery;
pub use domain_purchases::PurchaseListResponse;
pub use domain_purchases::SupplierSummary;
