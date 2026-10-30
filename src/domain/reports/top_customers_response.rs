use super::top_customer_entry::TopCustomerEntry;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopCustomersResponse {
    pub customers: Vec<TopCustomerEntry>,
    pub total_customers: u64,
    pub total_revenue_cents: i64,
}
