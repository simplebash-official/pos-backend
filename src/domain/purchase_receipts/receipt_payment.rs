use super::payment_source::PaymentSource;

use serde::Deserialize;
use utoipa::ToSchema;

/// Payment made as the receipt is recorded: paid in full, or part paid when the amount is less
/// than the total. Leave the whole `payment` out for a receipt bought on credit.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptPayment {
    pub source: PaymentSource,
    /// What is paid now, at most the receipt total.
    pub amount_cents: i64,
    /// The supplier-payment category the payment is filed under.
    pub category_key: String,
    /// This register's terminal. Required for the drawer and the owner's pocket, refused for
    /// the bank, a card and online.
    #[serde(default)]
    pub terminal_id: Option<String>,
    /// The open shift the cash is booked on; omitted only between shifts.
    #[serde(default)]
    pub session_key: Option<String>,
    /// `YYYY-MM-DD` the money left the account. Required for the bank, a card and online,
    /// refused for the drawer and the owner's pocket (cash is booked when it is recorded).
    #[serde(default)]
    pub paid_on: Option<String>,
}
