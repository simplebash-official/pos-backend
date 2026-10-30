use crate::domain::sessions::DrawerState;
use mongodb::bson::DateTime as BsonDateTime;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawerStateDocument {
    pub terminal_id: String,
    pub current_balance_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_session_key: Option<String>,
    pub last_closing_float_cents: i64,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl DrawerStateDocument {
    #[must_use]
    pub fn into_drawer_state(self) -> DrawerState {
        DrawerState {
            terminal_id: self.terminal_id,
            current_balance_cents: self.current_balance_cents,
            last_session_key: self.last_session_key,
            last_closing_float_cents: self.last_closing_float_cents,
            updated_at: self.updated_at.to_chrono(),
        }
    }
}
