//! The request a receipt is computed from.

use super::charges::{Allocation, BilledBy, ChargeKind};
use super::modes::{CostMode, PaymentMode, PricingMode};
use super::payment_source::PaymentSource;
use serde::{Deserialize, Serialize};

/// One product line of a receipt as typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcLine {
    pub product_key: Option<String>,
    /// True when the product is tracked by serial number.
    pub serialized: bool,
    /// `None` while the box is empty.
    pub quantity: Option<i64>,
    pub cost_mode: CostMode,
    /// 0 while nothing is typed.
    pub cost_cents: i64,
    pub pricing_mode: PricingMode,
    /// The typed price; only used in `price` mode. 0 while nothing is typed.
    pub selling_price_cents: i64,
    /// Per mille (255 is 25.5%); only used in `margin` mode.
    pub margin_tenths: Option<i64>,
    /// Only used in `margin` mode; 1 means no rounding.
    pub step_cents: Option<i64>,
    pub serial_numbers: Vec<String>,
}

/// One extra charge or discount on the receipt as typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcCharge {
    pub kind: Option<ChargeKind>,
    /// Always positive; a discount is its own kind. 0 while nothing is typed.
    pub amount_cents: i64,
    pub billed_by: Option<BilledBy>,
    /// Only used with two or more products.
    pub allocation: Option<Allocation>,
    /// Typed text; counted trimmed.
    pub description: String,
}

/// How the receipt's charges are paid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcChargesPayment {
    pub source: Option<PaymentSource>,
    /// `YYYY-MM-DD`, for the bank, a card and online.
    pub paid_on: String,
}

/// How the receipt itself is paid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcPayment {
    pub mode: PaymentMode,
    pub source: Option<PaymentSource>,
    /// Only used while `mode` is `part`.
    pub part_cents: i64,
    pub paid_on: String,
    pub category_key: Option<String>,
}

/// A whole receipt as typed, sent to be calculated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcReceiptRequest {
    pub supplier_key: Option<String>,
    pub reference_no: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub lines: Vec<CalcLine>,
    pub charges: Vec<CalcCharge>,
    pub charges_payment: CalcChargesPayment,
    pub payment: CalcPayment,
}
