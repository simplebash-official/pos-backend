use super::report_group::ReportGroup;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PATCH /cash-categories/{key}`. Absent fields are left unchanged.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCashCategoryRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// `Some(None)` is not expressible in JSON here, so clearing the Sinhala name sends an empty string.
    #[serde(default)]
    pub name_si: Option<String>,
    #[serde(default)]
    pub report_group: Option<ReportGroup>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub requires_receipt: Option<bool>,
    /// Zero clears the limit.
    #[serde(default)]
    pub requires_approval_over_cents: Option<i64>,
    /// Marks a cashier-made category as looked at.
    #[serde(default)]
    pub approve: Option<bool>,
}
