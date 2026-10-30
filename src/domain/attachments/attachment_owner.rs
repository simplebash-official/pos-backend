use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What a photo is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentOwner {
    CashMovement,
    Expense,
    Income,
}

impl AttachmentOwner {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            AttachmentOwner::CashMovement => "cash_movement",
            AttachmentOwner::Expense => "expense",
            AttachmentOwner::Income => "income",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cash_movement" => Some(AttachmentOwner::CashMovement),
            "expense" => Some(AttachmentOwner::Expense),
            "income" => Some(AttachmentOwner::Income),
            _ => None,
        }
    }
}
