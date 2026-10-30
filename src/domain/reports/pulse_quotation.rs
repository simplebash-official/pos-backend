use super::pulse_quotation_item::PulseQuotationItem;
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// An active quotation with items requiring stock replenishment or ready to fill.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseQuotation {
    pub key: String,
    pub quotation_number: String,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub total_cents: i64,
    pub created_at: DateTime<Utc>,
    pub valid_until: String,
    pub items: Vec<PulseQuotationItem>,
    pub is_ready_to_fill: bool,
}
