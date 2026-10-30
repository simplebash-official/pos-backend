use super::pulse_activity::PulseActivity;
use super::pulse_fast_mover::PulseFastMover;
use super::pulse_low_stock::PulseLowStock;
use super::pulse_quotation::PulseQuotation;
use super::pulse_ready_repair::PulseReadyRepair;
use super::pulse_stage::PulseStage;
use super::pulse_technician::PulseTechnician;

use serde::Serialize;
use utoipa::ToSchema;

/// Response of `GET /reports/dashboard-pulse`: everything the dashboard groups and totals.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DashboardPulse {
    pub active_repairs_count: i64,
    pub ready_repairs_count: i64,
    /// Estimate plus parts of every repair waiting for pickup.
    pub uncollected_ready_value_cents: i64,
    pub active_print_jobs_count: i64,
    pub repair_pipeline: Vec<PulseStage>,
    pub print_pipeline: Vec<PulseStage>,
    pub technicians: Vec<PulseTechnician>,
    pub fast_movers: Vec<PulseFastMover>,
    pub low_stock: Vec<PulseLowStock>,
    pub ready_repairs: Vec<PulseReadyRepair>,
    pub recent_activity: Vec<PulseActivity>,
    pub pending_stock_quotations: Vec<PulseQuotation>,
}
