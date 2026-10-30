use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Where a bill stands for one month.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BillStatus {
    /// Not yet inside its reminder window.
    Upcoming,
    /// Inside the reminder window, or due today, and not paid in full.
    Due,
    /// Past its due day and not paid in full.
    Overdue,
    Paid,
}
