use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /billing/invoices/{key}/void`. Unlike the old
/// cancel flow this collapsed from, `reason` is mandatory — a void always
/// reverses stock/payment effects that already happened, so it needs an
/// audit trail of why (see `InvoiceStatus::Voided`'s doc comment).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoidInvoiceRequest {
    /// Mandatory reason explaining why the invoice is being voided.
    pub reason: String,
}
