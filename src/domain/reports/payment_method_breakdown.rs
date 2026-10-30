use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Payment method distribution.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentMethodBreakdown {
    pub cash_cents: i64,
    pub card_cents: i64,
    pub online_cents: i64,
    pub credit_cents: i64,
}
