use super::aging_bucket::AgingBucket;
use super::aging_debtor::AgingDebtor;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceivablesAgingResponse {
    pub as_of: DateTime<Utc>,
    pub buckets: Vec<AgingBucket>,
    pub total_outstanding_cents: i64,
    pub total_invoices: u64,
    /// Customers with the largest 60+ day exposure (up to 10).
    pub top_debtors: Vec<AgingDebtor>,
}
