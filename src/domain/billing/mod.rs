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

mod domain_billing_part1;
mod domain_billing_part2;

pub use domain_billing_part1::BillingStats;
pub use domain_billing_part1::CompleteSaleResponse;
pub use domain_billing_part1::CreateCreditNoteItemRequest;
pub use domain_billing_part1::CreateCreditNoteRequest;
pub use domain_billing_part1::CreateSaleItemRequest;
pub use domain_billing_part1::CreateSaleRequest;
pub use domain_billing_part1::CreditNote;
pub use domain_billing_part1::CreditNoteItem;
pub use domain_billing_part1::CreditNoteListQuery;
pub use domain_billing_part1::CreditNoteListResponse;
pub use domain_billing_part1::CreditNoteStatus;
pub use domain_billing_part1::CustomerRef;
pub use domain_billing_part1::Invoice;
pub use domain_billing_part1::InvoiceItem;
pub use domain_billing_part1::InvoiceListQuery;
pub use domain_billing_part1::InvoiceListResponse;
pub use domain_billing_part1::InvoiceStatus;
pub use domain_billing_part1::ItemCondition;
pub use domain_billing_part1::ItemDisposition;
pub use domain_billing_part1::PaymentDetails;
pub use domain_billing_part1::PaymentListResponse;
pub use domain_billing_part1::PaymentRecord;
pub use domain_billing_part1::PricingAdjustments;
pub use domain_billing_part1::compute_is_overdue;
pub use domain_billing_part2::RecordPaymentRequest;
pub use domain_billing_part2::RefundBreakdownLeg;
pub use domain_billing_part2::ReturnReason;
pub use domain_billing_part2::SplitPayment;
pub use domain_billing_part2::StaffRef;
pub use domain_billing_part2::VoidCreditNoteRequest;
pub use domain_billing_part2::VoidInvoiceRequest;
