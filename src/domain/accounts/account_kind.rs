use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One of the shop's money accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Drawer,
    Bank,
    Card,
    Online,
}

impl AccountKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountKind::Drawer => "drawer",
            AccountKind::Bank => "bank",
            AccountKind::Card => "card",
            AccountKind::Online => "online",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "drawer" => Some(AccountKind::Drawer),
            "bank" => Some(AccountKind::Bank),
            "card" => Some(AccountKind::Card),
            "online" => Some(AccountKind::Online),
            _ => None,
        }
    }
}
