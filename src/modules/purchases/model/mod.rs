// Mongo document shape for the purchase / stock-intake history feature.
// Kept separate from `domain::purchases` (the API-facing types) so BSON
// concerns like `ObjectId` never leak into request/response payloads.

mod default_version;
mod purchase_document;
mod purchase_receipt_document;
mod receipt_charge_document;
mod receipt_figures;
mod receipt_settlement_document;
mod supplier_credit_document;

pub use purchase_document::PurchaseDocument;
pub use purchase_receipt_document::PurchaseReceiptDocument;
pub use receipt_charge_document::ReceiptChargeDocument;
pub use receipt_figures::ReceiptFigures;
pub use receipt_settlement_document::ReceiptSettlementDocument;
pub use supplier_credit_document::SupplierCreditDocument;
