use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

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
