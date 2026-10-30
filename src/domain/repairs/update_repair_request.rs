use super::repair_assignment::RepairAssignment;
use super::repair_customer::RepairCustomer;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PATCH /repairs/{id}`. Every field optional so a client sends
/// only what changed. Same `customerKey`-overrides-`customerName`/
/// `customerPhone` rule as `CreateRepairRequest` applies here too (see
/// `service::update_repair`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRepairRequest {
    /// Updated customer details.
    pub customer: Option<RepairCustomer>,
    /// Updated device model.
    pub device_model: Option<String>,
    /// Updated serial number or IMEI.
    pub serial_number: Option<String>,
    /// Updated issue description.
    pub issue_description: Option<String>,
    /// Updated promised-ready date (YYYY-MM-DD). Omit the key to leave it
    /// untouched; there is no way to clear it back to unset via this endpoint.
    pub promised_ready_at: Option<String>,
    /// Updated status.
    pub status: Option<String>,
    /// Updated customer price estimate in cents. Omit the key to leave the
    /// existing price untouched; there is no way to explicitly clear a price
    /// back to unset via this endpoint; nothing in the workflow needs that.
    pub estimated_cost_cents: Option<i64>,
    /// Updated material cost in cents.
    pub material_cost_cents: Option<i64>,
    /// Updated technician assignment.
    pub assignment: Option<RepairAssignment>,
}
