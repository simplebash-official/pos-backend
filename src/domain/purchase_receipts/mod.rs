// Pure business types for supplier receipts: one invoice's worth of stock received on a single
// visit or delivery. A receipt is the header; each product on it is a `Purchase` line. No I/O,
// no Mongo/Axum types beyond serde/utoipa derives.
//
// Nothing about what has been paid is stored on a receipt: the total, what has been paid and
// credited, what is still due and the status are worked out from its lines and from the
// settlements and credits that point at it every time it is read.

mod allocation;
mod billed_by;
mod charge_kind;
mod charges_payment;
mod create_receipt_request;
mod payment_kind;
mod payment_source;
mod purchase_receipt;
mod receipt_approval;
mod receipt_charge;
mod receipt_charge_request;
mod receipt_line_request;
mod receipt_list_query;
mod receipt_list_response;
mod receipt_payment;
mod receipt_settlement;
mod receipt_settlement_request;
mod receipt_status;

pub use allocation::Allocation;
pub use billed_by::BilledBy;
pub use charge_kind::ChargeKind;
pub use charges_payment::ChargesPayment;
pub use create_receipt_request::CreateReceiptRequest;
pub use payment_kind::PaymentKind;
pub use payment_source::PaymentSource;
pub use purchase_receipt::PurchaseReceipt;
pub use receipt_approval::ReceiptApproval;
pub use receipt_charge::ReceiptCharge;
pub use receipt_charge_request::ReceiptChargeRequest;
pub use receipt_line_request::ReceiptLineRequest;
pub use receipt_list_query::ReceiptListQuery;
pub use receipt_list_response::ReceiptListResponse;
pub use receipt_payment::ReceiptPayment;
pub use receipt_settlement::ReceiptSettlement;
pub use receipt_settlement_request::ReceiptSettlementRequest;
pub use receipt_status::ReceiptStatus;
