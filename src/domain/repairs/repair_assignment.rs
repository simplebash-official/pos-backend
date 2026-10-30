use serde::Deserialize;
use utoipa::ToSchema;

/// The `assignment{}` sub-object of `CreateRepairRequest`/
/// `UpdateRepairRequest` — every field is optional, so the whole object is
/// typically omitted entirely on an unassigned ticket. See `Repair`'s doc
/// comment for how `assignedEmployeeId` is resolved server-side.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairAssignment {
    /// Staff ID of assigned technician.
    #[serde(default)]
    pub assigned_employee_id: Option<String>,
    /// Name of assigned technician.
    #[serde(default)]
    pub assigned_employee_name: Option<String>,
    /// Commission split method ("percentage" or "fixed").
    #[serde(default)]
    pub split_type: Option<String>,
    /// Commission rate or fixed amount.
    #[serde(default)]
    pub split_value: Option<f64>,
}
