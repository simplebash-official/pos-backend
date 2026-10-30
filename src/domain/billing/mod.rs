// Pure business types for the billing feature — no I/O, no Mongo/Axum types
// beyond serde/utoipa derives. Mongo document shapes live in
// `modules::billing::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.
//
// Invoices and payments are append-only (see D3 in the migration plan): no
// general edit endpoint exists. `Invoice`/`PaymentRecord` are returned from
// create/list/get/void/close/record-payment only. Credit notes follow the
// same append-only posture: a credit note is created once as a single
// atomic write (never edited), with `void_credit_note` as the only
// after-the-fact reversal.

mod billing_stats;
mod complete_sale_response;
mod compute_is_overdue;
mod create_credit_note_item_request;
mod create_credit_note_request;
mod create_sale_item_request;
mod create_sale_request;
mod credit_note;
mod credit_note_item;
mod credit_note_list_query;
mod credit_note_list_response;
mod credit_note_status;
mod customer_ref;
mod default_version;
mod invoice;
mod invoice_item;
mod invoice_list_query;
mod invoice_list_response;
mod invoice_status;
mod item_condition;
mod item_disposition;
mod payment_details;
mod payment_list_response;
mod payment_record;
mod pricing_adjustments;
mod record_payment_request;
mod refund_breakdown_leg;
mod return_reason;
mod split_payment;
mod staff_ref;
mod void_credit_note_request;
mod void_invoice_request;

pub use billing_stats::BillingStats;
pub use complete_sale_response::CompleteSaleResponse;
pub use compute_is_overdue::compute_is_overdue;
pub use create_credit_note_item_request::CreateCreditNoteItemRequest;
pub use create_credit_note_request::CreateCreditNoteRequest;
pub use create_sale_item_request::CreateSaleItemRequest;
pub use create_sale_request::CreateSaleRequest;
pub use credit_note::CreditNote;
pub use credit_note_item::CreditNoteItem;
pub use credit_note_list_query::CreditNoteListQuery;
pub use credit_note_list_response::CreditNoteListResponse;
pub use credit_note_status::CreditNoteStatus;
pub use customer_ref::CustomerRef;
pub use invoice::Invoice;
pub use invoice_item::InvoiceItem;
pub use invoice_list_query::InvoiceListQuery;
pub use invoice_list_response::InvoiceListResponse;
pub use invoice_status::InvoiceStatus;
pub use item_condition::ItemCondition;
pub use item_disposition::ItemDisposition;
pub use payment_details::PaymentDetails;
pub use payment_list_response::PaymentListResponse;
pub use payment_record::PaymentRecord;
pub use pricing_adjustments::PricingAdjustments;
pub use record_payment_request::RecordPaymentRequest;
pub use refund_breakdown_leg::RefundBreakdownLeg;
pub use return_reason::ReturnReason;
pub use split_payment::SplitPayment;
pub use staff_ref::StaffRef;
pub use void_credit_note_request::VoidCreditNoteRequest;
pub use void_invoice_request::VoidInvoiceRequest;
