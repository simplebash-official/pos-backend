use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Why the supplier gave credit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreditReason {
    /// Fewer units arrived than were billed.
    ShortDelivery,
    /// Goods arrived or turned out damaged.
    Damaged,
    /// The billed price was higher than agreed.
    PriceDifference,
    Other,
}

impl CreditReason {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            CreditReason::ShortDelivery => "short_delivery",
            CreditReason::Damaged => "damaged",
            CreditReason::PriceDifference => "price_difference",
            CreditReason::Other => "other",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "short_delivery" => Some(CreditReason::ShortDelivery),
            "damaged" => Some(CreditReason::Damaged),
            "price_difference" => Some(CreditReason::PriceDifference),
            "other" => Some(CreditReason::Other),
            _ => None,
        }
    }
}
