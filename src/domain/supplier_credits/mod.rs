//! Pure business types for supplier credits: an amount a supplier takes off what the shop owes,
//! because goods were short or damaged or the price was wrong. No I/O, no Mongo/Axum types beyond
//! serde/utoipa derives.
//!
//! A credit lowers the supplier's balance and, when it names a receipt, that receipt's amount
//! due. It moves no stock: the shop receives only the units it accepts. A wrong credit is voided
//! by a reversing entry, never edited or deleted.

mod domain_supplier_credits;

pub use domain_supplier_credits::CreateSupplierCreditRequest;
pub use domain_supplier_credits::CreditReason;
pub use domain_supplier_credits::SupplierCredit;
pub use domain_supplier_credits::SupplierCreditListQuery;
pub use domain_supplier_credits::SupplierCreditListResponse;
