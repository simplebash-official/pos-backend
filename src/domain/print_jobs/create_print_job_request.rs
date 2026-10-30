use super::print_job_assignment::PrintJobAssignment;
use super::print_job_customer::PrintJobCustomer;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /print-jobs`. `customer.customerName`/`customerPhone` are
/// only used as-is when `customer.customerKey` is absent (a walk-in with no
/// linked account); when it resolves to a real customer,
/// `service::create_print_job` overrides both from that record instead of
/// trusting the payload — same rule as `repairs::CreateRepairRequest`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePrintJobRequest {
    /// Customer contact information.
    #[serde(default)]
    pub customer: PrintJobCustomer,
    /// Category of print job.
    pub job_type: String,
    /// Quantity of copies/items.
    pub quantity: i32,
    /// Optional date the job was promised ready (YYYY-MM-DD).
    #[serde(default)]
    pub promised_ready_at: Option<String>,
    /// Initial status (defaults to "received").
    #[serde(default)]
    pub status: Option<String>,
    /// Estimated total customer cost in cents.
    pub estimated_cost_cents: i64,
    /// Optional material cost in cents.
    #[serde(default)]
    pub material_cost_cents: Option<i64>,
    /// Staff assignment details.
    #[serde(default)]
    pub assignment: Option<PrintJobAssignment>,
}
