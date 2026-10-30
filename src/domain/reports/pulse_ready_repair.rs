use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// A repair ready for pickup and still waiting.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseReadyRepair {
    pub id: String,
    pub ticket_number: String,
    pub device_model: String,
    pub customer_name: String,
    pub customer_phone: String,
    pub created_at: DateTime<Utc>,
    pub days_waiting: i64,
    /// `critical` after three days, otherwise `warning`.
    pub severity: String,
    /// What the customer will be asked for.
    pub amount_cents: i64,
}
