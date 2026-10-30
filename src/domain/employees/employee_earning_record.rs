use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// One itemized commission line item for a single employee — the
/// server-computed replacement for the frontend's former mock earnings
/// ledger. Built by `modules::reports::service::employee_earnings`, not
/// persisted anywhere; recomputed from the underlying repair/print-job
/// documents on every request.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeEarningRecord {
    pub work_id: String,
    pub ticket_or_invoice_number: String,
    /// `"repair"` or `"print"`.
    pub work_type: String,
    pub description: String,
    pub customer_name: String,
    pub total_amount_cents: i64,
    pub cost_cents: i64,
    pub profit_cents: i64,
    pub split_type: String,
    pub split_value: f64,
    pub earned_amount_cents: i64,
    /// `"completed"` once the source ticket is delivered, `"pending"` otherwise.
    pub status: String,
    pub created_at: DateTime<Utc>,
}
