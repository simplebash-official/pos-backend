use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// An employee's job function — purely an HR/commission categorization,
/// carries no system-permission meaning (that's `domain::users::Role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmployeeRole {
    Technician,
    Printer,
    Sales,
    General,
}

impl EmployeeRole {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            EmployeeRole::Technician => "technician",
            EmployeeRole::Printer => "printer",
            EmployeeRole::Sales => "sales",
            EmployeeRole::General => "general",
        }
    }
}

impl std::str::FromStr for EmployeeRole {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "technician" => Ok(EmployeeRole::Technician),
            "printer" => Ok(EmployeeRole::Printer),
            "sales" => Ok(EmployeeRole::Sales),
            "general" => Ok(EmployeeRole::General),
            other => Err(format!("Unknown employee role: {other}")),
        }
    }
}
