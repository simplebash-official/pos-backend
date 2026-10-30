use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Where the money for an expense or a payment comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaidFrom {
    /// The cash drawer. Never entered here: a drawer payment is a cash movement.
    Drawer,
    Bank,
    Card,
    Online,
    /// Entered as a bill still to be paid.
    Unpaid,
}

impl PaidFrom {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            PaidFrom::Drawer => "drawer",
            PaidFrom::Bank => "bank",
            PaidFrom::Card => "card",
            PaidFrom::Online => "online",
            PaidFrom::Unpaid => "unpaid",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "drawer" => Some(PaidFrom::Drawer),
            "bank" => Some(PaidFrom::Bank),
            "card" => Some(PaidFrom::Card),
            "online" => Some(PaidFrom::Online),
            "unpaid" => Some(PaidFrom::Unpaid),
            _ => None,
        }
    }

    /// Whether this is a real account money can leave from (everything but `Unpaid`).
    #[must_use]
    pub fn is_account(&self) -> bool {
        !matches!(self, PaidFrom::Unpaid)
    }
}
