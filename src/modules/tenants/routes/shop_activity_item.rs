// Representation of a normalized business activity record for operator audit.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Details of a single business activity item.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShopActivityItem {
    pub id: String,
    /// Type of activity: "sale", "shift", "cash_movement", "credit_note", "repair", "print".
    pub kind: String,
    /// Reference number (e.g. invoice number, session number, movement number).
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cashier_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_cents: Option<i64>,
    pub description: String,
    pub created_at: DateTime<Utc>,
}
