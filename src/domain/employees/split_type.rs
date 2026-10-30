use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How an employee's commission on a job is calculated: a percentage of
/// profit, or a fixed amount per job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SplitType {
    Percentage,
    Fixed,
}

impl SplitType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            SplitType::Percentage => "percentage",
            SplitType::Fixed => "fixed",
        }
    }
}

impl std::str::FromStr for SplitType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "percentage" => Ok(SplitType::Percentage),
            "fixed" => Ok(SplitType::Fixed),
            other => Err(format!("Unknown split type: {other}")),
        }
    }
}
