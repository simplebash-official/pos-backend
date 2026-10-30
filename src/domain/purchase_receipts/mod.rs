//! Pure business types for supplier receipts: one invoice's worth of stock received on a single
//! visit or delivery. A receipt is the header; each product on it is a `Purchase` line. No I/O,
//! no Mongo/Axum types beyond serde/utoipa derives.
//!
//! Nothing about what has been paid is stored on a receipt: the total, what has been paid and
//! credited, what is still due and the status are worked out from its lines and from the
//! settlements and credits that point at it every time it is read.

mod domain_purchase_receipts;

pub use domain_purchase_receipts::Allocation;
pub use domain_purchase_receipts::BilledBy;
pub use domain_purchase_receipts::ChargeKind;
pub use domain_purchase_receipts::ChargesPayment;
pub use domain_purchase_receipts::CreateReceiptRequest;
pub use domain_purchase_receipts::PaymentKind;
pub use domain_purchase_receipts::PaymentSource;
pub use domain_purchase_receipts::PurchaseReceipt;
pub use domain_purchase_receipts::ReceiptApproval;
pub use domain_purchase_receipts::ReceiptCharge;
pub use domain_purchase_receipts::ReceiptChargeRequest;
pub use domain_purchase_receipts::ReceiptLineRequest;
pub use domain_purchase_receipts::ReceiptListQuery;
pub use domain_purchase_receipts::ReceiptListResponse;
pub use domain_purchase_receipts::ReceiptPayment;
pub use domain_purchase_receipts::ReceiptSettlement;
pub use domain_purchase_receipts::ReceiptSettlementRequest;
pub use domain_purchase_receipts::ReceiptStatus;
