//! Reasons a receipt cannot be saved yet.

use serde::{Deserialize, Serialize};

/// A reason the receipt cannot be saved yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum BlockerCode {
    SupplierMissing,
    DateInvalid,
    ReferenceTooLong,
    NoLines,
    ProductMissing,
    ProductDuplicate,
    QuantityMissing,
    CostLotMissing,
    CostUnitMissing,
    MarginMissing,
    StepMissing,
    MarginUnreachable,
    PriceMissing,
    SerialsMissing,
    SerialsDuplicate,
    ChargeKindMissing,
    ChargeAmountMissing,
    ChargeBilledByMissing,
    ChargeDiscountNotOnSupplierBill,
    ChargeDescriptionTooLong,
    ChargeAllocationMissing,
    ChargesSourceMissing,
    ChargesDayInvalid,
    DiscountsExceedLine,
    DiscountsExceedBill,
    PaymentSourceMissing,
    PaymentCategoryMissing,
    PartAmountMissing,
    PartIsWholeBill,
    PaymentDayInvalid,
}

/// One thing that stops the receipt from being saved. `line` / `charge` are zero-based positions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Blocker {
    pub code: BlockerCode,
    pub line: Option<usize>,
    pub charge: Option<usize>,
}
