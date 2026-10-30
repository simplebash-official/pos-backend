use super::pattern_bucket::PatternBucket;
use super::pattern_cell::PatternCell;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesPatternsResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    /// 7 × 24 = 168 cells, zero-filled, row-major (weekday then hour).
    pub cells: Vec<PatternCell>,
    pub by_weekday: Vec<PatternBucket>,
    pub by_hour: Vec<PatternBucket>,
    pub busiest: PatternCell,
}
