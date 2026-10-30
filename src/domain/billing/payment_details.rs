use super::split_payment::SplitPayment;

use serde::Deserialize;
use utoipa::ToSchema;

/// The `payment{}` sub-object of `CreateSaleRequest` — every field that
/// describes how the sale was (or will be) paid for, grouped together
/// rather than scattered across the request's top level.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentDetails {
    /// Selected payment method: `"cash"`, `"card"`, `"online"`, `"split"`, or `"credit"`.
    pub payment_method: String,
    /// Indicates if this transaction is recorded as unpaid credit.
    pub is_credit: bool,
    /// Total amount of money handed in by the customer in cents.
    #[serde(default)]
    pub amount_received_cents: Option<i64>,
    /// List of payment breakdown portions if payment method is "split".
    #[serde(default)]
    pub split_payments: Option<Vec<SplitPayment>>,
    /// Last 4 digits of payment card if paid by card.
    #[serde(default)]
    pub card_last4: Option<String>,
    /// Authorization code or reference number from card terminal.
    #[serde(default)]
    pub card_ref: Option<String>,
    /// Transaction ID or reference number for online payment.
    #[serde(default)]
    pub online_ref: Option<String>,
    /// Optional note or description for online transfer.
    #[serde(default)]
    pub online_note: Option<String>,
    /// Due date for credit invoice repayment (YYYY-MM-DD).
    #[serde(default)]
    pub due_date: Option<String>,
    /// How an up-front partial payment on a credit sale was taken:
    /// `"cash"` or `"card"`. Only meaningful when `is_credit` is true and
    /// `amount_received_cents` is a non-zero amount below the total — it
    /// selects the method recorded on the deposit's payment row. The
    /// invoice's own `payment_method` still stays `"credit"`.
    #[serde(default)]
    pub deposit_method: Option<String>,
}
