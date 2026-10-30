use super::payment_method_breakdown::PaymentMethodBreakdown;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// --- payment methods -------------------------------------------------------

/// `GET /reports/analytics/payment-methods` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsPaymentMethodsResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub breakdown: PaymentMethodBreakdown,
    pub total_cents: i64,
}
