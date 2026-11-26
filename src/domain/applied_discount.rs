//! A discount as it is stored on a job's item: what was typed, and what the server worked it out
//! to. Repair items and print job lines share it.

use pos_calc_core::item_discount::ItemDiscount;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The discount on one item: as typed (so it can be edited again) and what the server worked it
/// out to, stored together like the item's price and cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppliedDiscount {
    #[serde(flatten)]
    pub typed: ItemDiscount,
    /// What the discount comes to, in cents.
    pub amount_cents: i64,
}
