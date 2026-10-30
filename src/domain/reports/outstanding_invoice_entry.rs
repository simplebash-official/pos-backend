use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One credit invoice with unpaid balance.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutstandingInvoiceEntry {
    pub id: String,
    pub key: String,
    pub invoice_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    pub total_cents: i64,
    pub amount_paid_cents: i64,
    pub balance_due_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    pub is_overdue: bool,
    pub created_at: DateTime<Utc>,
}
