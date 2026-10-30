use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Where the money was received.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReceivedInto {
    /// The cash drawer. Never entered here: it is an add-cash movement.
    Drawer,
    Bank,
    Card,
    Online,
}

impl ReceivedInto {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ReceivedInto::Drawer => "drawer",
            ReceivedInto::Bank => "bank",
            ReceivedInto::Card => "card",
            ReceivedInto::Online => "online",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "drawer" => Some(ReceivedInto::Drawer),
            "bank" => Some(ReceivedInto::Bank),
            "card" => Some(ReceivedInto::Card),
            "online" => Some(ReceivedInto::Online),
            _ => None,
        }
    }
}
