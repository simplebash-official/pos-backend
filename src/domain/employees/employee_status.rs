use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmployeeStatus {
    Active,
    Inactive,
}

impl EmployeeStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            EmployeeStatus::Active => "active",
            EmployeeStatus::Inactive => "inactive",
        }
    }
}

impl std::str::FromStr for EmployeeStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(EmployeeStatus::Active),
            "inactive" => Ok(EmployeeStatus::Inactive),
            other => Err(format!("Unknown employee status: {other}")),
        }
    }
}
