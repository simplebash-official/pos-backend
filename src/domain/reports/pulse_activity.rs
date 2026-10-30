use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// One line of the live activity stream, newest first.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseActivity {
    /// `sale`, `repair` or `print`.
    pub kind: String,
    pub id: String,
    /// Invoice number or ticket number.
    pub reference: String,
    pub amount_cents: i64,
    pub created_at: DateTime<Utc>,
    pub payment_method: Option<String>,
    pub cashier_name: Option<String>,
    pub status: Option<String>,
    pub device_model: Option<String>,
    pub customer_name: Option<String>,
    pub job_type: Option<String>,
    pub quantity: Option<i64>,
}
