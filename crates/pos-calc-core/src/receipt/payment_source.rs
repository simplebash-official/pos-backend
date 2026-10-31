//! Where a payment is taken from.

use serde::{Deserialize, Serialize};

/// Where the money for a receipt payment comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum PaymentSource {
    Drawer,
    Bank,
    Card,
    Online,
    OwnerPocket,
}

impl PaymentSource {
    /// The name stored for this source.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            PaymentSource::Drawer => "drawer",
            PaymentSource::Bank => "bank",
            PaymentSource::Card => "card",
            PaymentSource::Online => "online",
            PaymentSource::OwnerPocket => "owner_pocket",
        }
    }

    /// Reads a stored name; `None` when it names no source.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "drawer" => Some(PaymentSource::Drawer),
            "bank" => Some(PaymentSource::Bank),
            "card" => Some(PaymentSource::Card),
            "online" => Some(PaymentSource::Online),
            "owner_pocket" => Some(PaymentSource::OwnerPocket),
            _ => None,
        }
    }
}
