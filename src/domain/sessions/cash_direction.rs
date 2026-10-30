use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Direction of a movement as far as the drawer's expected cash is concerned.
/// A cash drop is `Neutral` for profit reports but still lowers the drawer, which
/// the session totals track in their own `total_cash_drops_cents` bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CashDirection {
    Out,
    In,
    Neutral,
}

impl CashDirection {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            CashDirection::Out => "out",
            CashDirection::In => "in",
            CashDirection::Neutral => "neutral",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "out" => Some(CashDirection::Out),
            "in" => Some(CashDirection::In),
            "neutral" => Some(CashDirection::Neutral),
            _ => None,
        }
    }
}
