//! What the warranty dashboard and report need to know about repair and print jobs.

use chrono::NaiveDate;
use std::collections::HashMap;

/// One rework job's cost to the shop (its parts and recorded work costs; the customer pays
/// nothing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReworkCost {
    /// The shop-local day the rework was opened.
    pub opened_on: NaiveDate,
    pub cost_cents: i64,
}

/// The employee a job was assigned to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobTechnician {
    pub employee_id: String,
    pub name: String,
}

/// What the warranty report needs from one print job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintJobFacts {
    /// Absent when nobody was assigned.
    pub technician: Option<JobTechnician>,
    /// Each line's description, by line key.
    pub line_descriptions: HashMap<String, String>,
}
