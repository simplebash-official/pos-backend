use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Whose balance an opening belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BalanceSubject {
    Supplier,
    Staff,
}

impl BalanceSubject {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            BalanceSubject::Supplier => "supplier",
            BalanceSubject::Staff => "staff",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "supplier" => Some(BalanceSubject::Supplier),
            "staff" => Some(BalanceSubject::Staff),
            _ => None,
        }
    }
}
