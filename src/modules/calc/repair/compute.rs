use super::{CalcRepairRequest, RepairTotals};
use crate::core::error::AppResult;

/// Works out a repair job's totals with the shared calculation crate.
pub fn compute(request: &CalcRepairRequest) -> AppResult<RepairTotals> {
    Ok(pos_calc_core::repair::compute(request)?)
}
