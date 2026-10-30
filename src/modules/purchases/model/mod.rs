//! Mongo document shape for the purchase / stock-intake history feature.
//! Kept separate from `domain::purchases` (the API-facing types) so BSON
//! concerns like `ObjectId` never leak into request/response payloads.

mod purchases_model;

pub use purchases_model::PurchaseDocument;
pub use purchases_model::PurchaseReceiptDocument;
pub use purchases_model::ReceiptChargeDocument;
pub use purchases_model::ReceiptFigures;
pub use purchases_model::ReceiptSettlementDocument;
pub use purchases_model::SupplierCreditDocument;
