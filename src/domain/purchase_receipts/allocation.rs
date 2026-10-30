use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How an extra charge is spread over the products on the receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Allocation {
    /// In proportion to what each line costs.
    Value,
    /// In proportion to how many units each line has.
    Quantity,
}

impl Allocation {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Allocation::Value => "value",
            Allocation::Quantity => "quantity",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "value" => Some(Allocation::Value),
            "quantity" => Some(Allocation::Quantity),
            _ => None,
        }
    }
}
