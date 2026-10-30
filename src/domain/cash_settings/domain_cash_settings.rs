use serde::Deserialize;
use utoipa::ToSchema;

/// Most quick amounts the cash form shows.
pub const MAX_QUICK_AMOUNTS: usize = 8;

/// Largest amount a rule can hold (Rs. 1,000,000).
pub const MAX_RULE_CENTS: i64 = 100_000_000;

/// Amount shortcuts shown on the cash form, as shipped (Rs. 100, 500, 1,000, 5,000).
pub const STARTING_QUICK_AMOUNTS_CENTS: [i64; 4] = [10_000, 50_000, 100_000, 500_000];

/// Seconds a cashier can undo their own movement, as shipped.
pub const STARTING_UNDO_WINDOW_SECONDS: i64 = 120;

/// Fewest and most seconds the undo window can be set to.
pub const UNDO_WINDOW_RANGE: (i64, i64) = (10, 3600);

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
