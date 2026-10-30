use super::customers_feed_data::CustomersFeedData;
use super::engine_metadata::EngineMetadata;
use super::overview_feed_data::OverviewFeedData;
use super::profit_feed_data::ProfitFeedData;
use super::sales_feed_data::SalesFeedData;
use super::staff_feed_data::StaffFeedData;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Unified response payload for the Engine Feed.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EngineFeedResponse {
    pub section: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub meta: EngineMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<OverviewFeedData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sales: Option<SalesFeedData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profit: Option<ProfitFeedData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customers: Option<CustomersFeedData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staff: Option<StaffFeedData>,
}
