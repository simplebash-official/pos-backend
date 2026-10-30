use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Who asks for the money of an extra charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BilledBy {
    /// It is on the supplier's bill: it adds to (or, for a discount, takes from) what is owed them.
    Supplier,
    /// Someone else is paid (courier, clearing agent, porter): paid now, not owed to the supplier.
    ThirdParty,
}

impl BilledBy {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            BilledBy::Supplier => "supplier",
            BilledBy::ThirdParty => "third_party",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "supplier" => Some(BilledBy::Supplier),
            "third_party" => Some(BilledBy::ThirdParty),
            _ => None,
        }
    }
}
