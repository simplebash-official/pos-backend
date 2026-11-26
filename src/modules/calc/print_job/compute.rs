use super::{CalcPrintJobRequest, PrintJobPreview};
use crate::core::error::AppResult;

/// Works out a print job's totals, each line and what blocks saving, with the shared
/// calculation crate.
pub fn compute(request: &CalcPrintJobRequest) -> AppResult<PrintJobPreview> {
    Ok(pos_calc_core::print_job::compute(request)?)
}
