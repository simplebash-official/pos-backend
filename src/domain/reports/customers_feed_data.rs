use super::receivables_aging_response::ReceivablesAgingResponse;
use super::top_customers_response::TopCustomersResponse;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Customers section feed bundle.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomersFeedData {
    pub top_customers: TopCustomersResponse,
    pub receivables_aging: ReceivablesAgingResponse,
}
