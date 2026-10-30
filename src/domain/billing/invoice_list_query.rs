use serde::Deserialize;
use utoipa::IntoParams;

/// Query parameters for listing and filtering invoices.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct InvoiceListQuery {
    /// Search term matching invoice number or customer name/phone.
    pub search: Option<String>,
    /// Filter by status ("pending", "partially_paid", "paid", "voided",
    /// "closed"), or the synthetic value "overdue" (translated server-side
    /// to `status in (pending, partially_paid) AND due_date < today`).
    pub status: Option<String>,
    /// Filter by customer key.
    pub customer_key: Option<String>,
    /// `"paid"` (status == "paid" AND NOT credit) or `"credit"` (is_credit
    /// OR status in (pending, partially_paid)) — the same compound rule
    /// `BillingStats.outstandingCreditCents` uses, exposed here so the
    /// Sales & Invoices History screen's Paid/Credit toggle can filter
    /// server-side.
    pub payment_status: Option<String>,
    /// Exact match on payment method ("cash", "card", "online", "split").
    pub payment_method: Option<String>,
    /// Only `"today"` is meaningful — scopes to `created_at` within
    /// `shop_today_range()`. Any other value (or absence) means all time.
    pub date_preset: Option<String>,
    /// Filter by register shift session key.
    pub session_key: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Page item limit.
    pub limit: Option<u64>,
}
