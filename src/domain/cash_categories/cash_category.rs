use super::report_group::ReportGroup;
use super::review_status::ReviewStatus;
use crate::domain::sessions::CashMovementType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A category a cash movement can be filed under, for one movement type.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashCategory {
    pub id: String,
    pub key: String,
    pub movement_type: CashMovementType,
    pub name: String,
    /// Optional Sinhala name; search matches both languages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_si: Option<String>,
    pub report_group: ReportGroup,
    /// Shipped with the app (seeded). System categories can be hidden or renamed but not merged away.
    pub is_system: bool,
    pub review_status: ReviewStatus,
    /// Pinned categories always lead the picker, ahead of usage ranking.
    pub pinned: bool,
    pub sort_order: i64,
    /// Hidden categories leave the picker but stay on old movements and reports.
    pub active: bool,
    pub requires_receipt: bool,
    /// A payout above this amount needs a manager PIN. `None` means no per-category limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_approval_over_cents: Option<i64>,
    /// How many movements use this category; the picker ranks by this. Counted at read time.
    pub usage_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_name_snapshot: Option<String>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
