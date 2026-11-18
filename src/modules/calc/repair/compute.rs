use super::{CalcRepairRequest, RepairPreview};
use crate::core::error::AppResult;

/// Works out a repair job's totals, each item's discount and what blocks saving, with the
/// shared calculation crate.
pub fn compute(request: &CalcRepairRequest) -> AppResult<RepairPreview> {
    Ok(pos_calc_core::repair::compute(request)?)
}
