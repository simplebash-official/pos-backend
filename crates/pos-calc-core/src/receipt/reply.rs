//! What computing a receipt returns.

use super::blocker::Blocker;
use serde::{Deserialize, Serialize};

/// What a line earns at its selling price.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcMargin {
    pub profit_each_cents: i64,
    pub lot_profit_cents: i64,
    /// Tenths of a percent (167 is 16.7%).
    pub margin_tenths: i64,
    pub below_cost: bool,
}

/// The figures for one receipt line: landed cost, price and batches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcReceiptLineReply {
    /// What the line costs before extra charges.
    pub goods_cents: i64,
    /// Goods plus the line's share of the extra charges that are filled in far enough to count.
    pub landed_cents: i64,
    /// The landed total per unit to the nearest cent, for display; 0 without a quantity.
    pub unit_cost_cents: i64,
    /// The cost as a price each: exact only. `None` when the lot does not divide evenly.
    pub cost_as_unit_cents: Option<i64>,
    /// The cost as the whole-lot price.
    pub cost_as_lot_cents: i64,
    /// The selling price each that will be saved; 0 while it cannot be worked out yet.
    pub selling_price_cents: i64,
    /// In margin mode: the price the margin alone asks for, before rounding up.
    pub price_without_rounding_cents: Option<i64>,
    /// True when rounding up moved the price above what the margin alone asks for.
    pub price_rounded_up: bool,
    pub margin: Option<CalcMargin>,
    /// The margin (tenths) to start from when switching price -> margin; `None` when the price
    /// earns a margin the margin box cannot hold.
    pub margin_tenths_if_switched: Option<i64>,
}

/// Short name for [`CalcReceiptLineReply`].
pub type CalcLineReply = CalcReceiptLineReply;

/// The receipt's totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcReceiptSummary {
    pub goods_cents: i64,
    /// On the supplier's bill, discounts taken off.
    pub extra_billed_cents: i64,
    /// What the supplier's bill comes to.
    pub total_cents: i64,
    /// Paid to someone else; not owed to the supplier.
    pub paid_to_others_cents: i64,
    pub paying_now_cents: i64,
    pub left_owed_cents: i64,
    /// All the money leaving today.
    pub cash_out_today_cents: i64,
}

/// The calculated receipt: every line, the summary and the blockers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcReceiptReply {
    pub lines: Vec<CalcReceiptLineReply>,
    pub summary: CalcReceiptSummary,
    /// True while an extra charge is half filled in, so its cost is not counted yet.
    pub charges_incomplete: bool,
    /// True when cash leaves the drawer, which is what a manager's approval is for.
    pub involves_cash: bool,
    /// In the order the form should tell the person; empty means Save may go ahead.
    pub blockers: Vec<Blocker>,
}
