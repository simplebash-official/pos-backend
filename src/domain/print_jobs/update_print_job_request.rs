use super::print_job_assignment::PrintJobAssignment;
use super::print_job_customer::PrintJobCustomer;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PATCH /print-jobs/{id}`. Same `customerKey`-overrides-
/// `customerName`/`customerPhone` rule as `CreatePrintJobRequest` applies
/// here too (see `service::update_print_job`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePrintJobRequest {
    /// Updated customer details.
    pub customer: Option<PrintJobCustomer>,
    /// Updated print job category.
    pub job_type: Option<String>,
    /// Updated quantity.
    pub quantity: Option<i32>,
    /// Updated promised-ready date (YYYY-MM-DD). Omit the key to leave it
    /// untouched; there is no way to clear it back to unset via this endpoint.
    pub promised_ready_at: Option<String>,
    /// Updated lifecycle status.
    pub status: Option<String>,
    /// Updated customer charge estimate in cents.
    pub estimated_cost_cents: Option<i64>,
    /// Updated material cost in cents.
    pub material_cost_cents: Option<i64>,
    /// Updated staff assignment.
    pub assignment: Option<PrintJobAssignment>,
}
