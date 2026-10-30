use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What kind of extra charge this is. A discount lowers the cost; the rest raise it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChargeKind {
    Shipping,
    Customs,
    Handling,
    Insurance,
    Other,
    /// Taken off by the supplier (early payment, a price break). Counts as a negative charge.
    Discount,
}

impl ChargeKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ChargeKind::Shipping => "shipping",
            ChargeKind::Customs => "customs",
            ChargeKind::Handling => "handling",
            ChargeKind::Insurance => "insurance",
            ChargeKind::Other => "other",
            ChargeKind::Discount => "discount",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "shipping" => Some(ChargeKind::Shipping),
            "customs" => Some(ChargeKind::Customs),
            "handling" => Some(ChargeKind::Handling),
            "insurance" => Some(ChargeKind::Insurance),
            "other" => Some(ChargeKind::Other),
            "discount" => Some(ChargeKind::Discount),
            _ => None,
        }
    }
}
