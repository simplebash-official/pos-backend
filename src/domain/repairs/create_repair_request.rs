use super::repair_assignment::RepairAssignment;
use super::repair_customer::RepairCustomer;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /repairs` and `PUT /repairs/{id}`. `ticketNumber` is never
/// client-supplied — reserved server-side on create only (see
/// `service::create_repair`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRepairRequest {
    /// Customer contact information.
    #[serde(default)]
    pub customer: RepairCustomer,
    /// Device model/brand.
    pub device_model: String,
    /// Optional serial number or IMEI.
    #[serde(default)]
    pub serial_number: Option<String>,
    /// Reported issue or symptom description.
    pub issue_description: String,
    /// Optional date the job was promised ready (YYYY-MM-DD).
    #[serde(default)]
    pub promised_ready_at: Option<String>,
    /// Initial status (defaults to "received").
    #[serde(default)]
    pub status: Option<String>,
    /// Estimated charge to customer in cents. Optional: a ticket can be
    /// created before a technician has diagnosed the device and quoted a
    /// price; see `service::validate_price_required_for_status`.
    #[serde(default)]
    pub estimated_cost_cents: Option<i64>,
    /// Optional cost of parts/materials in cents.
    #[serde(default)]
    pub material_cost_cents: Option<i64>,
    /// Optional technician assignment.
    #[serde(default)]
    pub assignment: Option<RepairAssignment>,
}
