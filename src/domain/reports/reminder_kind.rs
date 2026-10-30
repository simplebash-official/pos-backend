use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What a single reminder is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReminderKind {
    /// A credit invoice whose payment due date has passed.
    CreditOverdue,
    /// A credit invoice due within the "due soon" window.
    CreditDueSoon,
    /// A repair or print job whose promised-ready date has passed.
    JobOverdue,
    /// A repair or print job promised ready within the "due soon" window.
    JobDueSoon,
}
