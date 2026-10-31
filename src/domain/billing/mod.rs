//! Pure business types for the billing feature — no I/O, no Mongo/Axum types
//! beyond serde/utoipa derives. Mongo document shapes live in
//! `modules::billing::model` and convert into these before a handler wraps
//! them in `core::response::ApiResponse<T>`.
//!
//! Invoices and payments are append-only (see D3 in the migration plan): no
//! general edit endpoint exists. `Invoice`/`PaymentRecord` are returned from
//! create/list/get/void/close/record-payment only. Credit notes follow the
//! same append-only posture: a credit note is created once as a single
//! atomic write (never edited), with `void_credit_note` as the only
//! after-the-fact reversal.

mod credit_note;
mod invoice;
mod payment;
mod sale;

pub use credit_note::CreateCreditNoteItemRequest;
pub use credit_note::CreateCreditNoteRequest;
pub use credit_note::CreditNote;
pub use credit_note::CreditNoteItem;
pub use credit_note::CreditNoteListQuery;
pub use credit_note::CreditNoteListResponse;
pub use credit_note::CreditNoteStatus;
pub use credit_note::ItemCondition;
pub use credit_note::ItemDisposition;
pub use credit_note::RefundBreakdownLeg;
pub use credit_note::ReturnReason;
pub use credit_note::VoidCreditNoteRequest;
pub use invoice::BillingStats;
pub use invoice::CustomerRef;
pub use invoice::Invoice;
pub use invoice::InvoiceItem;
pub use invoice::InvoiceListQuery;
pub use invoice::InvoiceListResponse;
pub use invoice::InvoiceStatus;
pub use invoice::PricingAdjustments;
pub use invoice::StaffRef;
pub use invoice::VoidInvoiceRequest;
pub use invoice::compute_is_overdue;
pub use payment::PaymentDetails;
pub use payment::PaymentListResponse;
pub use payment::PaymentRecord;
pub use payment::RecordPaymentRequest;
pub use payment::SplitPayment;
pub use sale::CompleteSaleResponse;
pub use sale::CreateSaleItemRequest;
pub use sale::CreateSaleRequest;
