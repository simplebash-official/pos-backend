use mongodb::bson::doc;
use serde::Serialize;
use utoipa::ToSchema;

/// One day of the "bills per day" chart (Sri Lanka calendar day).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DayUsage {
    /// `YYYY-MM-DD`, Asia/Colombo.
    pub date: String,
    pub bills: u64,
    pub sales_cents: i64,
}
