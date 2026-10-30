use serde::Deserialize;
use utoipa::ToSchema;

/// The `assignment{}` sub-object of `CreatePrintJobRequest`/
/// `UpdatePrintJobRequest` — same shape as `repairs::RepairAssignment`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobAssignment {
    /// User ID of employee assigned to job.
    #[serde(default)]
    pub assigned_employee_id: Option<String>,
    /// Name of employee assigned to job.
    #[serde(default)]
    pub assigned_employee_name: Option<String>,
    /// Commission split type ("percentage" or "fixed").
    #[serde(default)]
    pub split_type: Option<String>,
    /// Commission rate or fixed amount.
    #[serde(default)]
    pub split_value: Option<f64>,
}
