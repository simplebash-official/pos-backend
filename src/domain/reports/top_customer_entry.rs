use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopCustomerEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    pub customer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    /// True for the single collapsed "Walk-in" (no linked customer) bucket.
    pub is_walk_in: bool,
    pub invoice_count: u64,
    pub revenue_cents: i64,
    pub discount_cents: i64,
    pub gross_profit_cents: i64,
    /// Current all-time balance owed by this customer (from the customer record).
    pub outstanding_cents: i64,
    pub last_purchase_at: DateTime<Utc>,
}
