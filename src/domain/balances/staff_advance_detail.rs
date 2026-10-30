use super::advance_given::AdvanceGiven;
use super::advance_recovery::AdvanceRecovery;
use super::staff_advance::StaffAdvance;

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffAdvanceDetail {
    pub summary: StaffAdvance,
    pub given: Vec<AdvanceGiven>,
    pub recoveries: Vec<AdvanceRecovery>,
}
