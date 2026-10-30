use serde::Serialize;
use utoipa::ToSchema;

/// One payment method breakdown row in the Z-Report.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZReportPaymentMethodSummary {
    pub method: String,
    pub count: u64,
    pub total_cents: i64,
}
