use serde::Serialize;
use utoipa::ToSchema;

/// One category's total inside a section of a report.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    pub category_key: String,
    pub category_name: String,
    pub total_cents: i64,
}
