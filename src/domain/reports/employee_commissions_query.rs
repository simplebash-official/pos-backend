use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// Query parameters for Employee Commissions report.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeCommissionsQuery {
    pub preset: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// Filters to one employee by key. Grouping is by key, not display name
    /// — two employees sharing a name never merge, and a rename doesn't
    /// split history across two buckets (see
    /// `reports::repository::commissions::aggregate_employee_commissions`).
    pub employee_key: Option<String>,
}
