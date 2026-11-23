//! The warranty desk's headline numbers (`GET /warranties/stats`), worked out on the server for
//! the shop-local today.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Days an open claim may wait before it counts as waiting too long.
pub const CLAIM_SLOW_AFTER_DAYS: i64 = 7;

/// The warranty desk at a glance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyStats {
    /// Warranties still covering their item today and not held by a claim (ending soon
    /// included).
    pub in_force_count: u64,
    /// Of those, the ones in their last week.
    pub ending_soon_count: u64,
    /// The last end day that still counts as ending soon (`YYYY-MM-DD`).
    pub ending_soon_until: String,
    /// Claims not yet resolved.
    pub open_claim_count: u64,
    /// Open claims waiting more than `CLAIM_SLOW_AFTER_DAYS` days, or with a supplier past the
    /// day they gave.
    pub late_claim_count: u64,
    /// What rework jobs opened this shop-local month (and not cancelled) cost the shop in
    /// parts.
    pub rework_cost_this_month_cents: i64,
}
