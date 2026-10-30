use serde::Serialize;
use utoipa::ToSchema;

/// One technician or printer operator's load.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseTechnician {
    pub employee_key: String,
    pub name: String,
    /// `technician` or `printer`.
    pub role: String,
    pub active_jobs_count: i64,
    pub completed_today_count: i64,
    /// Five active jobs is full (100).
    pub capacity_percent: i64,
    /// `available`, `busy` or `overloaded`.
    pub status: String,
    /// The first active job: its device and ticket.
    pub current_device_model: Option<String>,
    pub current_ticket_number: Option<String>,
}
