//! How a line is costed, priced and paid for.

use serde::{Deserialize, Serialize};

/// Whether a line's cost is per unit or for the whole lot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum CostMode {
    /// `cost_cents` is the price of one unit.
    Unit,
    /// `cost_cents` is the supplier's price for the whole lot.
    Lot,
}

/// Whether a line's selling price is typed or worked out from a margin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum PricingMode {
    /// The selling price is typed.
    Price,
    /// The selling price is worked out from `margin_tenths`, rounded up to `step_cents`.
    Margin,
}

/// How much of the receipt is paid now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum PaymentMode {
    Credit,
    Full,
    Part,
}
