use super::staff_advance::StaffAdvance;

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffAdvancesResponse {
    pub staff: Vec<StaffAdvance>,
}
