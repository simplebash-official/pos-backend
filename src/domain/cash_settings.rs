// Pure business types for the shop's drawer rules. No I/O, no framework types beyond serde/utoipa
// derives. The values live in one stored row; the shipped starting values (`STARTING_*`) only seed
// that row and are never read as a fallback.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Seconds a cashier can undo their own movement, as shipped.
pub const STARTING_UNDO_WINDOW_SECONDS: i64 = 120;
/// Amount shortcuts shown on the cash form, as shipped (Rs. 100, 500, 1,000, 5,000).
pub const STARTING_QUICK_AMOUNTS_CENTS: [i64; 4] = [10_000, 50_000, 100_000, 500_000];

/// Fewest and most seconds the undo window can be set to.
pub const UNDO_WINDOW_RANGE: (i64, i64) = (10, 3600);
/// Most quick amounts the cash form shows.
pub const MAX_QUICK_AMOUNTS: usize = 8;
/// Largest amount a rule can hold (Rs. 1,000,000).
pub const MAX_RULE_CENTS: i64 = 100_000_000;

/// The shop's drawer rules.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashSettings {
    /// How long the person who recorded a movement can undo it without a manager.
    pub undo_window_seconds: i64,
    /// A payout above this needs a manager PIN whatever its category; absent means no shop-wide limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_threshold_cents: Option<i64>,
    /// Amount shortcuts under the amount field, smallest first.
    pub quick_amounts_cents: Vec<i64>,
    /// What each drawer should hold after excess is dropped; absent means no target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub float_target_cents: Option<i64>,
    pub version: i64,
    pub updated_at: DateTime<Utc>,
}

/// Body for `PUT /cash-settings`. The whole set is sent every time; an absent optional value clears it.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCashSettingsRequest {
    pub undo_window_seconds: i64,
    #[serde(default)]
    pub approval_threshold_cents: Option<i64>,
    pub quick_amounts_cents: Vec<i64>,
    #[serde(default)]
    pub float_target_cents: Option<i64>,
}
