use super::payment_source::PaymentSource;

use serde::Deserialize;
use utoipa::ToSchema;

/// How the third-party extra charges are paid, as the receipt is recorded. Same sources as the
/// goods payment; the money is filed under the shop's "Stock charges" category.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChargesPayment {
    pub source: PaymentSource,
    /// This register's terminal. Required for the drawer and the owner's pocket, refused otherwise.
    #[serde(default)]
    pub terminal_id: Option<String>,
    /// The open shift the cash is booked on; omitted only between shifts.
    #[serde(default)]
    pub session_key: Option<String>,
    /// `YYYY-MM-DD` the money left the account. Required for the bank, a card and online,
    /// refused for the drawer and the owner's pocket.
    #[serde(default)]
    pub paid_on: Option<String>,
}
