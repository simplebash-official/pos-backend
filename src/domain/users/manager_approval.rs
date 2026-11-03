//! A manager or owner signing off an action at the counter.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A manager or owner PIN and the reason the action is allowed. The PIN is checked on the
/// server on every request, whatever the screen already checked.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManagerApproval {
    pub manager_pin: String,
    /// Why; stored with the action and shown in its history.
    pub reason: String,
}

impl std::fmt::Debug for ManagerApproval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagerApproval")
            .field("manager_pin", &"<redacted>")
            .field("reason", &self.reason)
            .finish()
    }
}

/// Who approved an action, once their PIN was accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedBy {
    pub user_id: String,
    pub name: String,
    /// The trimmed reason they gave.
    pub reason: String,
}
