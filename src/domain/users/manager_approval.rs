//! A manager or owner signing off an action at the counter.

use super::Role;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A manager or owner PIN and the reason the action is allowed. Only Staff send one: a signed-in
/// Admin or Manager approves their own action, and a request from them must not carry it. The
/// PIN is checked on the server on every request, whatever the screen already checked.
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

/// Who approved an action: the PIN holder, or the signed-in Admin or Manager approving it
/// themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedBy {
    pub user_id: String,
    pub name: String,
    /// The trimmed reason a PIN approval gave; `None` when the signed-in manager approved it
    /// themselves.
    pub reason: Option<String>,
}

/// The signed-in user an approval is resolved for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalCaller {
    pub user_id: String,
    pub name: String,
    /// The role in the caller's token; `None` is treated like Staff.
    pub role: Option<Role>,
}

impl ApprovalCaller {
    /// Whether this caller approves their own actions: an Admin (the owner) or a Manager.
    #[must_use]
    pub fn self_approves(&self) -> bool {
        self.role.is_some_and(Role::self_approves)
    }
}
