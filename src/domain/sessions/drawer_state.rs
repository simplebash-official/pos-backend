use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Persistent ledger of cash surviving across sessions for a terminal.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DrawerState {
    pub terminal_id: String,
    pub current_balance_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_session_key: Option<String>,
    pub last_closing_float_cents: i64,
    pub updated_at: DateTime<Utc>,
}
