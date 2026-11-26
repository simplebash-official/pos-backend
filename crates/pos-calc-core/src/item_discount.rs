//! The discount typed on one priced line, and the arithmetic every kind of job shares for it.
//! Which lines may carry one is each job's own rule (`repair`, `print_job`).

use serde::{Deserialize, Serialize};

use crate::calculations::round_half_up;

/// The most a percentage discount can be, in basis points (100%).
const MAX_BASIS_POINTS: u32 = 10_000;

/// What was typed as one item's discount: a share of the item's price, or cents. A percentage
/// travels as whole basis points (12.5% is 1250) so no floating point touches the money.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum ItemDiscount {
    /// A share of the item's price, 1 to 10000 basis points.
    #[serde(rename_all = "camelCase")]
    Percentage { basis_points: u32 },
    /// An amount off the item's price, at least 1 cent.
    #[serde(rename_all = "camelCase")]
    Fixed { cents: i64 },
}

/// Said when a discount is outside what any line can carry.
pub(crate) const OUT_OF_RANGE_MESSAGE: &str =
    "A discount must be between 0.01% and 100%, or at least 1 cent.";

impl ItemDiscount {
    /// Whether any line could carry this discount: 0.01% to 100%, or at least 1 cent.
    pub(crate) fn is_in_range(self) -> bool {
        match self {
            ItemDiscount::Percentage { basis_points } => {
                (1..=MAX_BASIS_POINTS).contains(&basis_points)
            }
            ItemDiscount::Fixed { cents } => cents >= 1,
        }
    }

    /// What an in-range discount takes off a line priced at `gross_cents`, or `None` when it is
    /// more than that price (a line with no price takes no discount).
    pub(crate) fn amount_off(self, gross_cents: i64) -> Option<i64> {
        match self {
            ItemDiscount::Percentage { basis_points } => (gross_cents > 0).then(|| {
                let cents =
                    round_half_up(i128::from(gross_cents) * i128::from(basis_points), 10_000);
                i64::try_from(cents).expect("a share of an in-range price fits in cents")
            }),
            ItemDiscount::Fixed { cents } => (cents <= gross_cents).then_some(cents),
        }
    }
}
